//! [`FileAssetLocator`]: finds the on-disk texture file for a normalized
//! key by walking each of a mod's loaded folders' `Textures/` directory,
//! and reads a located file's bytes back.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rim_analyzer::extract::textures;
use rim_analyzer::infra::engine_enumeration_order;
use rim_session::ports::{
    AboutImage, AssetLocator, DefSourceError, MAX_TEXTURE_BYTES, TextureBytes, TextureFormat,
};

/// [`textures::IMAGE_EXTENSIONS`] minus `dds` — the extensions
/// [`FileAssetLocator::locate_texture`]'s second pass searches, once the
/// first (DDS-anywhere) pass has found nothing.
const NON_DDS_IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// Walks `mod_loaded_folders`' `Textures/` directories looking for a file
/// whose normalized key ([`rim_analyzer::extract::textures::normalize`])
/// equals the requested one. Stateless: every call re-walks from disk.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileAssetLocator;

impl FileAssetLocator {
    /// Builds the locator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The first file under any of `folders`' own `Textures/` directory
    /// whose normalized key matches `texture_key`, restricted to
    /// `extensions` — folders searched in the order given (index 0
    /// highest priority), each folder's own files in
    /// [`engine_enumeration_order`] (the order RimWorld's own
    /// `ModContentLoader<T>.LoadAllForMod` reads them in, via the
    /// identical `GetAllFilesForMod` file-listing mechanism this crate's
    /// `Defs/`/`Patches/` scan already models).
    fn first_match(folders: &[PathBuf], texture_key: &str, extensions: &[&str]) -> Option<PathBuf> {
        for folder in folders {
            let textures_dir = folder.join("Textures");
            if !textures_dir.is_dir() {
                continue;
            }
            for path in engine_enumeration_order(&textures_dir, extensions) {
                let Ok(relative) = path.strip_prefix(&textures_dir) else {
                    continue;
                };
                if textures::normalize(relative).as_deref() == Some(texture_key) {
                    return Some(path);
                }
            }
        }
        None
    }
}

impl AssetLocator for FileAssetLocator {
    /// **A `.dds` anywhere shadows every non-`.dds` file with the same
    /// key**, whatever folder either sits in (ground-truthed against
    /// the decompiled `ModContentLoader<T>.LoadAllForMod`, whose own
    /// `ddsFiles` set is consulted before any other candidate, for the
    /// whole mod, not per folder). So the first pass searches every
    /// folder for a `.dds` match before the second pass ever looks at a
    /// non-`.dds` extension; within each pass, folder priority (and, for
    /// two same-priority-folder same-extension ties, engine enumeration
    /// order) decides the winner.
    fn locate_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        if let Some(dds) = Self::first_match(mod_loaded_folders, texture_key, &["dds"]) {
            return Ok(Some(dds));
        }
        Ok(Self::first_match(
            mod_loaded_folders,
            texture_key,
            &NON_DDS_IMAGE_EXTENSIONS,
        ))
    }

    /// Only the PNG/JPEG pass of [`Self::locate_texture`], so a `.dds`
    /// beside the image does not hide it. Same walk, same exact
    /// normalized-key comparison: the key is never joined into a path.
    fn locate_non_dds_texture(
        &self,
        mod_loaded_folders: &[PathBuf],
        texture_key: &str,
    ) -> Result<Option<PathBuf>, DefSourceError> {
        Ok(Self::first_match(
            mod_loaded_folders,
            texture_key,
            &NON_DDS_IMAGE_EXTENSIONS,
        ))
    }

    fn read_texture(&self, path: &Path) -> Result<TextureBytes, DefSourceError> {
        // Bounds the read itself rather than checking `fs::metadata` first
        // and trusting it — a metadata-then-read sequence is a TOCTOU: the
        // file (or whatever the path now points to, on a platform where
        // it can be swapped) can grow between the two calls, so the
        // second, unbounded `fs::read` would still allocate however much
        // an attacker-controlled path actually holds. `take(MAX + 1)`
        // reads at most one byte past the cap regardless of the file's
        // real size, so an oversized file costs at most `MAX_TEXTURE_BYTES
        // + 1` bytes of memory before being rejected, never more.
        let mut file = fs::File::open(path).map_err(|error| DefSourceError::Io {
            file: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_TEXTURE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| DefSourceError::Io {
                file: path.to_path_buf(),
                message: error.to_string(),
            })?;
        let actual_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_bytes > MAX_TEXTURE_BYTES {
            return Err(DefSourceError::TooLarge {
                file: path.to_path_buf(),
                max_bytes: MAX_TEXTURE_BYTES,
                actual_bytes,
            });
        }

        let format =
            TextureFormat::sniff(&bytes).ok_or_else(|| DefSourceError::UnsupportedFormat {
                file: path.to_path_buf(),
            })?;
        Ok(TextureBytes { format, bytes })
    }

    /// A plain path join plus an existence check — the identical
    /// resolution `Verse.ModMetaData.PreviewImagePath`'s own
    /// `File.Exists` check performs. No case-insensitive fallback beyond
    /// what the filesystem itself gives for free (case-insensitive on
    /// Windows/NTFS, case-sensitive elsewhere) — see [`AboutImage`]'s own
    /// doc comment for why that's deliberate, not an oversight.
    fn locate_about_image(&self, mod_root: &Path, image: AboutImage) -> Option<PathBuf> {
        let path = match image {
            AboutImage::Preview => mod_root.join("About").join("Preview.png"),
            AboutImage::Icon => mod_root.join("About").join("ModIcon.png"),
        };
        path.is_file().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn finds_a_texture_by_its_normalized_key() {
        let dir = tempdir().expect("tempdir");
        let textures = dir.path().join("Textures").join("UI");
        std::fs::create_dir_all(&textures).expect("create Textures/UI");
        std::fs::write(textures.join("Foo.png"), b"fake png bytes").expect("seed texture");
        let locator = FileAssetLocator::new();

        let found = locator
            .locate_texture(&[dir.path().to_path_buf()], "ui/foo")
            .expect("must succeed");

        assert_eq!(found, Some(textures.join("Foo.png")));
    }

    #[test]
    fn returns_none_when_no_folder_has_a_matching_texture() {
        let dir = tempdir().expect("tempdir");
        let locator = FileAssetLocator::new();

        let found = locator
            .locate_texture(&[dir.path().to_path_buf()], "ui/missing")
            .expect("must succeed");

        assert_eq!(found, None);
    }

    /// A `.dds` shadows a `.png` with the same key even when the `.dds`
    /// lives in a **lower**-priority folder than the `.png` — "a DDS
    /// anywhere shadows a PNG" is a whole-mod rule, not folder-scoped.
    /// Regression: before this change, `locate_texture` picked the first
    /// folder with *any* matching extension, so the higher-priority
    /// folder's `.png` would have won instead.
    #[test]
    fn a_dds_in_a_lower_priority_folder_shadows_a_png_in_a_higher_one() {
        let dir = tempdir().expect("tempdir");
        let high = dir.path().join("high");
        let low = dir.path().join("low");
        std::fs::create_dir_all(high.join("Textures")).expect("create high/Textures");
        std::fs::create_dir_all(low.join("Textures")).expect("create low/Textures");
        std::fs::write(high.join("Textures").join("Foo.png"), b"png bytes")
            .expect("seed the higher-priority png");
        std::fs::write(low.join("Textures").join("Foo.dds"), b"dds bytes")
            .expect("seed the lower-priority dds");
        let locator = FileAssetLocator::new();

        let found = locator
            .locate_texture(&[high, low.clone()], "foo")
            .expect("must succeed");

        assert_eq!(found, Some(low.join("Textures").join("Foo.dds")));
    }

    /// The image copy beside a `.dds` is found by the non-DDS locate, and
    /// the DDS-first locate still returns the `.dds`.
    #[test]
    fn the_non_dds_locate_finds_the_png_beside_a_dds() {
        let dir = tempdir().expect("tempdir");
        let textures = dir.path().join("Textures");
        std::fs::create_dir_all(&textures).expect("create Textures");
        std::fs::write(textures.join("Foo.dds"), b"dds bytes").expect("seed dds");
        std::fs::write(textures.join("Foo.png"), b"png bytes").expect("seed png");
        let locator = FileAssetLocator::new();
        let folders = [dir.path().to_path_buf()];

        let image = locator.locate_non_dds_texture(&folders, "foo").expect("ok");
        let loaded = locator.locate_texture(&folders, "foo").expect("ok");

        assert_eq!(image, Some(textures.join("Foo.png")));
        assert_eq!(loaded, Some(textures.join("Foo.dds")));
    }

    /// A key with only a `.dds` has no image copy, and a key that tries to
    /// climb out of `Textures/` matches nothing (it is compared, never
    /// joined into a path).
    #[test]
    fn the_non_dds_locate_ignores_a_lone_dds_and_a_traversal_key() {
        let dir = tempdir().expect("tempdir");
        let textures = dir.path().join("Textures");
        std::fs::create_dir_all(&textures).expect("create Textures");
        std::fs::write(textures.join("Only.dds"), b"dds bytes").expect("seed dds");
        std::fs::write(dir.path().join("secret.png"), b"png bytes").expect("seed outside");
        let locator = FileAssetLocator::new();
        let folders = [dir.path().to_path_buf()];

        assert_eq!(
            locator
                .locate_non_dds_texture(&folders, "only")
                .expect("ok"),
            None
        );
        assert_eq!(
            locator
                .locate_non_dds_texture(&folders, "../secret")
                .expect("ok"),
            None
        );
    }

    /// With no `.dds` anywhere, the plain priority-order PNG match still
    /// resolves — the base case the DDS-first pass must fall through to.
    #[test]
    fn a_lone_png_is_found() {
        let dir = tempdir().expect("tempdir");
        let textures = dir.path().join("Textures").join("UI");
        std::fs::create_dir_all(&textures).expect("create Textures/UI");
        std::fs::write(textures.join("Foo.png"), b"fake png bytes").expect("seed texture");
        let locator = FileAssetLocator::new();

        let found = locator
            .locate_texture(&[dir.path().to_path_buf()], "ui/foo")
            .expect("must succeed");

        assert_eq!(found, Some(textures.join("Foo.png")));
    }

    #[test]
    fn earlier_folders_win_over_later_ones() {
        let dir = tempdir().expect("tempdir");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir_all(first.join("Textures")).expect("create first/Textures");
        std::fs::create_dir_all(second.join("Textures")).expect("create second/Textures");
        std::fs::write(first.join("Textures").join("Foo.png"), b"first").expect("seed first");
        std::fs::write(second.join("Textures").join("Foo.png"), b"second").expect("seed second");
        let locator = FileAssetLocator::new();

        let found = locator
            .locate_texture(&[first.clone(), second], "foo")
            .expect("must succeed");

        assert_eq!(found, Some(first.join("Textures").join("Foo.png")));
    }

    /// A real, minimal 1x1 grayscale PNG — the same fixture bytes used by
    /// `crates/rim-io/tests/fixtures/merge_game`'s `ModA` texture.
    const ONE_PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3A,
        0x7E, 0x9B, 0x55, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60,
        0x00, 0x02, 0x00, 0x00, 0x05, 0x00, 0x01, 0xE9, 0xFA, 0xDC, 0xD8, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn reads_a_real_png_back_and_sniffs_its_format() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("wall.png");
        std::fs::write(&path, ONE_PIXEL_PNG).expect("seed the PNG fixture");
        let locator = FileAssetLocator::new();

        let texture = locator.read_texture(&path).expect("must succeed");

        assert_eq!(texture.format, TextureFormat::Png);
        assert_eq!(texture.bytes, ONE_PIXEL_PNG);
    }

    #[test]
    fn reads_a_jpeg_header_stub_and_sniffs_its_format() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("wall.jpg");
        // Only the magic bytes matter to `TextureFormat::sniff`; a full
        // JPEG stream isn't needed to prove the sniff+read path works.
        std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]).expect("seed the JPEG stub");
        let locator = FileAssetLocator::new();

        let texture = locator.read_texture(&path).expect("must succeed");

        assert_eq!(texture.format, TextureFormat::Jpeg);
    }

    #[test]
    fn a_file_over_the_cap_is_refused() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("huge.png");
        let oversized = vec![0u8; usize::try_from(MAX_TEXTURE_BYTES).expect("fits usize") + 1];
        std::fs::write(&path, &oversized).expect("seed an oversized file");
        let locator = FileAssetLocator::new();

        let error = locator
            .read_texture(&path)
            .expect_err("a file over the cap must be refused");

        assert!(matches!(
            error,
            DefSourceError::TooLarge {
                max_bytes: MAX_TEXTURE_BYTES,
                ..
            }
        ));
    }

    #[test]
    fn locates_an_existing_preview_png() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("About")).expect("create About/");
        std::fs::write(dir.path().join("About").join("Preview.png"), b"fake png").expect("seed");
        let locator = FileAssetLocator::new();

        let found = locator.locate_about_image(dir.path(), AboutImage::Preview);

        assert_eq!(found, Some(dir.path().join("About").join("Preview.png")));
    }

    #[test]
    fn locates_an_existing_mod_icon_png() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("About")).expect("create About/");
        std::fs::write(dir.path().join("About").join("ModIcon.png"), b"fake png").expect("seed");
        let locator = FileAssetLocator::new();

        let found = locator.locate_about_image(dir.path(), AboutImage::Icon);

        assert_eq!(found, Some(dir.path().join("About").join("ModIcon.png")));
    }

    #[test]
    fn a_missing_preview_png_is_none() {
        let dir = tempdir().expect("tempdir");
        let locator = FileAssetLocator::new();

        let found = locator.locate_about_image(dir.path(), AboutImage::Preview);

        assert_eq!(found, None);
    }

    /// `File.Exists` (what `Verse.ModMetaData` itself checks) is
    /// case-insensitive on Windows/NTFS — a plain `Path::join` + existence
    /// check therefore matches the engine on this platform without any
    /// case-folding logic of its own.
    #[cfg(windows)]
    #[test]
    fn a_lowercase_about_and_preview_are_found_on_windows() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("about")).expect("create about/");
        std::fs::write(dir.path().join("about").join("preview.png"), b"fake png").expect("seed");
        let locator = FileAssetLocator::new();

        let found = locator.locate_about_image(dir.path(), AboutImage::Preview);

        assert!(found.is_some(), "NTFS is case-insensitive");
    }

    #[test]
    fn an_unrecognized_format_is_rejected() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("not-a-texture.png");
        std::fs::write(&path, b"this is not an image").expect("seed a non-image file");
        let locator = FileAssetLocator::new();

        let error = locator
            .read_texture(&path)
            .expect_err("unrecognized magic bytes must be rejected");

        assert!(matches!(error, DefSourceError::UnsupportedFormat { .. }));
    }
}
