//! Normalizes a texture file path (as found under a `Textures/` folder)
//! into the key RimWorld actually resolves by: extension-independent,
//! case-insensitive, `/`-separated.

use std::path::Path;

/// Image file extensions RimWorld loads as textures — the single source
/// of truth shared with `infra::mod_scan`'s `Textures/` file walk.
///
/// `dds` is included: without it, `texPath` existence-checking against
/// `Indices.texture_owners` would silently miss every DDS-only-shipped
/// texture. On a real install, active mods ship more `.dds` files under their
/// own loaded `Textures/` folders than png/jpg/jpeg ones, so this is not a
/// rare case. The walk that builds `scanned_mod.textures` already visits
/// every file in `Textures/` regardless of extension (see
/// `infra::mod_scan::scan_folder`), so recognizing one more extension here
/// costs nothing extra in IO, only the final filter check.
pub const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "dds"];

/// How many leading bytes of a `.dds` file [`classify_dds`] needs: the
/// standard 128-byte header (4-byte magic plus the 124-byte `DDS_HEADER`
/// struct) plus the 20-byte `DDS_HEADER_DXT10` extension a `DX10` FourCC
/// carries — read unconditionally, since a caller doing a single bounded
/// read up front doesn't yet know whether the file has the extension.
pub const DDS_HEADER_BYTES: usize = 148;

const DDS_MAGIC: [u8; 4] = *b"DDS ";
const DDS_HEADER_SIZE: u32 = 124;
/// `DDS_PIXELFORMAT.dwFlags`' `DDPF_FOURCC` bit.
const DDPF_FOURCC: u32 = 0x4;

/// What [`classify_dds`] found in a `.dds` file's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsVerdict {
    /// `true` when RimWorld's own `ModDdsLoader.TryLoadDds` cannot decode
    /// this file (see [`classify_dds`]'s own doc comment for the exact
    /// rule) — the base game shows the bad-texture placeholder for it.
    pub undecodable: bool,
    pub width: u32,
    pub height: u32,
    /// The pixel format's four-character-code tag (`DXT5`, `DX10`, ...).
    /// Empty when the header was too short to read one at all.
    pub fourcc: String,
}

fn read_u32_le(header: &[u8], offset: usize) -> Option<u32> {
    header
        .get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_fourcc(header: &[u8]) -> String {
    match header.get(84..88) {
        Some(raw) if raw != [0, 0, 0, 0] => String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string(),
        _ => String::new(),
    }
}

/// The only three FourCCs `Verse.DdsPixelFormat.ToTextureFormat` accepts
/// for a compressed pixel format (`IsDxt1`/`IsDxt5`/`IsBc7`, the last
/// keyed on the literal FourCC `"DX10"` — the DX10 extended-header marker,
/// which this decompiled engine maps to `TextureFormat.BC7` unconditionally,
/// never inspecting the DXGI format field the extended header actually
/// carries). Any other compressed FourCC makes `ToTextureFormat` throw
/// `NotSupportedException` immediately, **before** a `Texture2D` is ever
/// constructed — a width/height check never even runs for it. Directly
/// decompiled and confirmed, not inferred from the managed DLL's outward
/// behaviour alone (unlike the non-multiple-of-4 case below).
const SUPPORTED_COMPRESSED_FOURCCS: [&str; 3] = ["DXT1", "DXT5", "DX10"];

/// Classifies a `.dds` file's leading bytes ([`DDS_HEADER_BYTES`] worth,
/// though `header` may be shorter — a truncated file — without ever
/// panicking, every read bounds-checked). Ground-truthed against
/// decompiled `Verse.ModDdsLoader.TryLoadDds`/`DdsPixelFormat
/// .ToTextureFormat`/`.IsCompressed`/`.IsUnsupportedCompressedFormat` for
/// the magic/size/compression/FourCC-support half; the non-multiple-of-4
/// failure itself is Unity constructor behaviour, unconfirmable from the
/// managed DLL alone, but matched 68-for-68 against a real play-test's
/// own game log:
///
/// - A bad magic (not `"DDS "`) or a header size other than 124 makes the
///   game's own reader throw `InvalidDataException` — undecodable.
/// - Otherwise, when the pixel format has `DDPF_FOURCC` set with a
///   non-zero FourCC: a FourCC outside `SUPPORTED_COMPRESSED_FOURCCS`
///   (DXT1, DXT5, DX10 — the only three `ToTextureFormat` recognizes)
///   makes it throw `NotSupportedException` immediately, undecodable
///   regardless of the image's own dimensions; one of those three still
///   fails, separately, when its width or height isn't a multiple of 4,
///   inside Unity's own `Texture2D` constructor (only reached once the
///   FourCC itself was accepted).
/// - An uncompressed pixel format is always fine, whatever its dimensions.
#[must_use]
pub fn classify_dds(header: &[u8]) -> DdsVerdict {
    let width = read_u32_le(header, 16).unwrap_or(0);
    let height = read_u32_le(header, 12).unwrap_or(0);
    let fourcc = read_fourcc(header);

    let valid_magic = header.get(0..4) == Some(&DDS_MAGIC);
    let valid_size = read_u32_le(header, 4) == Some(DDS_HEADER_SIZE);
    if !valid_magic || !valid_size {
        return DdsVerdict {
            undecodable: true,
            width,
            height,
            fourcc,
        };
    }

    let pixel_format_flags = read_u32_le(header, 80).unwrap_or(0);
    let fourcc_value = read_u32_le(header, 84).unwrap_or(0);
    let is_compressed = pixel_format_flags & DDPF_FOURCC != 0 && fourcc_value != 0;
    let is_supported_fourcc = SUPPORTED_COMPRESSED_FOURCCS.contains(&fourcc.as_str());
    let undecodable = is_compressed
        && (!is_supported_fourcc || !width.is_multiple_of(4) || !height.is_multiple_of(4));

    DdsVerdict {
        undecodable,
        width,
        height,
        fourcc,
    }
}

/// Normalizes `relative_path` — a path already relative to a `Textures/`
/// folder, e.g. `Things/Building/Wall.png` — into its lookup key:
/// extension stripped, backslashes normalized to `/`, lowercased.
///
/// Returns `None` for paths that aren't image files RimWorld would load.
#[must_use]
pub fn normalize(relative_path: &Path) -> Option<String> {
    let extension = relative_path.extension()?.to_str()?;
    if !IMAGE_EXTENSIONS
        .iter()
        .any(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return None;
    }

    let without_extension = relative_path.with_extension("");
    let key = without_extension
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_extension_and_lowercases() {
        let key = normalize(&PathBuf::from("Things/Building/Wall.PNG")).unwrap();
        assert_eq!(key, "things/building/wall");
    }

    #[test]
    fn normalizes_backslash_separators() {
        let key = normalize(&PathBuf::from(r"Things\Building\Wall.png")).unwrap();
        assert_eq!(key, "things/building/wall");
    }

    #[test]
    fn accepts_jpg_and_jpeg() {
        assert!(normalize(&PathBuf::from("a.jpg")).is_some());
        assert!(normalize(&PathBuf::from("a.jpeg")).is_some());
    }

    /// A mod shipping only a `.dds` copy of a texture (no `.png` alongside
    /// it) must still count as "shipped" — real installs carry tens of
    /// thousands of such files.
    #[test]
    fn accepts_dds() {
        let key = normalize(&PathBuf::from("Things/Building/Wall.dds")).unwrap();
        assert_eq!(key, "things/building/wall");
    }

    #[test]
    fn rejects_non_image_extensions() {
        assert_eq!(normalize(&PathBuf::from("readme.txt")), None);
    }

    #[test]
    fn rejects_extensionless_paths() {
        assert_eq!(normalize(&PathBuf::from("noext")), None);
    }

    /// Builds a synthetic, byte-exact DDS header (never a binary fixture
    /// file, per this crate's own convention for untrusted-input parsers):
    /// magic, header size, dimensions, and a pixel format carrying
    /// `pf_flags`/`fourcc`. `128` bytes — the DX10 extension, when a test
    /// needs one, is appended separately.
    fn dds_header(width: u32, height: u32, pf_flags: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[0..4].copy_from_slice(&DDS_MAGIC);
        header[4..8].copy_from_slice(&DDS_HEADER_SIZE.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        header[80..84].copy_from_slice(&pf_flags.to_le_bytes());
        header[84..88].copy_from_slice(fourcc);
        header
    }

    #[test]
    fn dxt5_with_non_multiple_of_4_width_is_undecodable() {
        let header = dds_header(65, 64, DDPF_FOURCC, b"DXT5");
        let verdict = classify_dds(&header);
        assert!(verdict.undecodable);
        assert_eq!(verdict.width, 65);
        assert_eq!(verdict.fourcc, "DXT5");
    }

    #[test]
    fn dxt1_multiple_of_4_is_fine() {
        let header = dds_header(64, 64, DDPF_FOURCC, b"DXT1");
        assert!(!classify_dds(&header).undecodable);
    }

    #[test]
    fn uncompressed_rgba_with_odd_size_is_fine() {
        // No `DDPF_FOURCC` flag set — an uncompressed format, whatever its
        // dimensions.
        let header = dds_header(65, 33, 0, b"\0\0\0\0");
        assert!(!classify_dds(&header).undecodable);
    }

    #[test]
    fn bad_magic_is_undecodable() {
        let mut header = dds_header(64, 64, DDPF_FOURCC, b"DXT1");
        header[0] = b'X';
        let verdict = classify_dds(&header);
        assert!(verdict.undecodable);
    }

    #[test]
    fn truncated_header_is_undecodable_not_a_panic() {
        let verdict = classify_dds(b"DD");
        assert!(verdict.undecodable);
        assert_eq!(verdict.width, 0);
        assert_eq!(verdict.height, 0);
        assert_eq!(verdict.fourcc, "");
    }

    #[test]
    fn bc7_via_dx10_header_with_odd_height_is_undecodable() {
        let mut header = dds_header(64, 65, DDPF_FOURCC, b"DX10");
        header.extend_from_slice(&[0u8; 20]); // the DX10 extension header
        assert_eq!(header.len(), DDS_HEADER_BYTES);
        let verdict = classify_dds(&header);
        assert!(verdict.undecodable);
        assert_eq!(verdict.fourcc, "DX10");
    }

    /// Regression: `Verse.DdsPixelFormat.ToTextureFormat` throws
    /// `NotSupportedException` for any compressed FourCC outside
    /// DXT1/DXT5/DX10, **regardless of dimensions** — a width/height check
    /// never even runs for it, since the exception fires before a
    /// `Texture2D` is ever constructed. A dimensions-only check missed
    /// this entirely: an unsupported FourCC at dimensions that are
    /// multiples of 4 (which the divisibility check alone would call
    /// fine) must still classify as undecodable.
    #[test]
    fn an_unsupported_compressed_fourcc_is_undecodable_even_at_valid_dimensions() {
        let header = dds_header(64, 64, DDPF_FOURCC, b"DXT3");
        let verdict = classify_dds(&header);
        assert!(verdict.undecodable, "DXT3 is not one of DXT1/DXT5/DX10");
        assert_eq!(verdict.fourcc, "DXT3");
    }
}
