//! [`ReadTexture`]: reads one active mod's texture file back by its
//! normalized path — the texture-override change summary's backend
//! half.

use std::path::PathBuf;

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::ports::{AssetLocator, DefSourceError, TextureBytes};

/// Everything that can go wrong reading a mod's texture back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadTextureError {
    /// `mod_id` (compared by [`ModId::base`]) isn't in the active mod list.
    #[error("{0} is not an active mod")]
    UnknownMod(ModId),
    /// `texture_path` isn't among `mod_id`'s loaded folders.
    #[error("{texture_path} was not found among {mod_id}'s files")]
    TextureNotFound {
        /// The mod that was searched.
        mod_id: ModId,
        /// The normalized texture path that wasn't found.
        texture_path: String,
    },
    /// Locating or reading the located file failed.
    #[error(transparent)]
    Asset(#[from] DefSourceError),
}

/// What [`ReadTexture::execute`] returns: the texture's bytes and sniffed
/// format, plus the on-disk path they were actually read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTextureOutput {
    /// Where the file actually lives on disk.
    pub path: PathBuf,
    /// The file's bytes and sniffed format.
    pub texture: TextureBytes,
}

/// Reads one active mod's texture file by its normalized path (see
/// `rim_analyzer::extract::textures::normalize`), for the desktop app's
/// `read_texture` command.
pub struct ReadTexture<Locator> {
    locator: Locator,
}

impl<Locator: AssetLocator> ReadTexture<Locator> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(locator: Locator) -> Self {
        Self { locator }
    }

    /// Resolves `mod_id`'s loaded folders from `session`'s report
    /// (matching by [`ModId::base`], so a `_steam`-suffixed id or its
    /// base resolves the same mod), locates `texture_path` among them via
    /// [`AssetLocator::locate_texture`], and reads the located file back.
    ///
    /// # Errors
    ///
    /// Returns [`ReadTextureError::UnknownMod`] when `mod_id` isn't
    /// active, [`ReadTextureError::TextureNotFound`] when the locator
    /// finds no match, or [`ReadTextureError::Asset`] when locating or
    /// reading the located file fails (I/O, over the size cap, or an
    /// unrecognized format).
    pub fn execute(
        &self,
        session: &Session,
        mod_id: &ModId,
        texture_path: &str,
    ) -> Result<ReadTextureOutput, ReadTextureError> {
        let target = mod_id.base();
        let mod_entry = session
            .report()
            .mods
            .iter()
            .find(|m| m.id.base() == target)
            .ok_or_else(|| ReadTextureError::UnknownMod(mod_id.clone()))?;

        let located = self
            .locator
            .locate_texture(&mod_entry.loaded_folders, texture_path)?;
        let Some(path) = located else {
            return Err(ReadTextureError::TextureNotFound {
                mod_id: mod_entry.id.clone(),
                texture_path: texture_path.to_string(),
            });
        };

        let texture = self.locator.read_texture(&path)?;
        Ok(ReadTextureOutput { path, texture })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::ports::TextureFormat;
    use crate::test_support::{FakeAssetLocator, session_fixture};

    fn png_bytes() -> TextureBytes {
        TextureBytes {
            format: TextureFormat::Png,
            bytes: vec![0x89, 0x50, 0x4E, 0x47],
        }
    }

    #[test]
    fn reads_a_located_texture_back() {
        let session = session_fixture(&["a"]);
        let path = PathBuf::from("Mods/a/Textures/things/wall.png");
        let locator =
            FakeAssetLocator::new(BTreeMap::from([("things/wall".to_string(), path.clone())]))
                .with_bytes(path.clone(), png_bytes());
        let use_case = ReadTexture::new(locator);

        let output = use_case
            .execute(&session, &ModId::new("a"), "things/wall")
            .expect("must succeed");

        assert_eq!(output.path, path);
        assert_eq!(output.texture, png_bytes());
    }

    #[test]
    fn a_steam_suffixed_active_mod_resolves_by_its_base_id() {
        let session = session_fixture(&["a_steam"]);
        let path = PathBuf::from("Mods/a/Textures/things/wall.png");
        let locator =
            FakeAssetLocator::new(BTreeMap::from([("things/wall".to_string(), path.clone())]))
                .with_bytes(path.clone(), png_bytes());
        let use_case = ReadTexture::new(locator);

        let output = use_case
            .execute(&session, &ModId::new("a"), "things/wall")
            .expect("the base id must resolve the _steam-suffixed active mod");

        assert_eq!(output.path, path);
    }

    #[test]
    fn an_inactive_mod_is_unknown() {
        let session = session_fixture(&["a"]);
        let use_case = ReadTexture::new(FakeAssetLocator::default());

        let result = use_case.execute(&session, &ModId::new("not.active"), "things/wall");

        assert!(
            matches!(result, Err(ReadTextureError::UnknownMod(id)) if id == ModId::new("not.active"))
        );
    }

    #[test]
    fn a_texture_the_locator_cannot_find_is_reported() {
        let session = session_fixture(&["a"]);
        let use_case = ReadTexture::new(FakeAssetLocator::default());

        let result = use_case.execute(&session, &ModId::new("a"), "things/missing");

        assert!(matches!(result,
            Err(ReadTextureError::TextureNotFound { texture_path, .. })
                if texture_path == "things/missing"
        ));
    }

    #[test]
    fn a_read_failure_on_the_located_file_propagates() {
        let session = session_fixture(&["a"]);
        let path = PathBuf::from("Mods/a/Textures/things/wall.png");
        let locator =
            FakeAssetLocator::new(BTreeMap::from([("things/wall".to_string(), path.clone())]))
                .with_read_error(
                    path.clone(),
                    DefSourceError::UnsupportedFormat { file: path },
                );
        let use_case = ReadTexture::new(locator);

        let result = use_case.execute(&session, &ModId::new("a"), "things/wall");

        assert!(matches!(
            result,
            Err(ReadTextureError::Asset(
                DefSourceError::UnsupportedFormat { .. }
            ))
        ));
    }
}
