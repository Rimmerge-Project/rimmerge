//! [`ReadDefTexture`]: reads the image behind one face of a def's
//! resolved graphic. The key must be one the def's own resolution
//! produced, so a forged key can read nothing the def does not show.

use std::path::Path;

use rim_analyzer::domain::{Conflict, ModId};
use rim_resolve::domain::DefRef;

use super::key::TextureKey;
use super::model::{Availability, DefGraphic, Face};
use super::resolve::{ResolveDefGraphic, ResolveDefGraphicError};
use crate::Session;
use crate::ports::{AssetLocator, DefSourceError, DefSourceReader, TextureBytes};
use crate::use_cases::PreviewUnreadable;

/// Where a shown image came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    /// The very file the game loads.
    Direct,
    /// The PNG/JPEG beside a `.dds` the game loads instead.
    PngSibling,
}

/// What reading one face gave. An expected miss is a value, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefTexture {
    /// An image to show.
    Image {
        /// The bytes and their sniffed format.
        texture: TextureBytes,
        /// The mod whose file this is.
        owner: ModId,
        /// Whether it is the game's own file or a sibling copy.
        from: ImageSource,
    },
    /// The game loads a `.dds` with no image copy; there is no decoder.
    DdsNotPreviewable {
        /// The mod shipping the `.dds`.
        owner: ModId,
    },
    /// The `.dds` is one the game cannot decode (it shows a placeholder).
    UndecodableInGame {
        /// The mod shipping the `.dds`.
        owner: ModId,
    },
    /// Built into the game or an asset bundle; never read.
    NotViewable {
        /// Whether the key was found nowhere and the built-in index is too
        /// small to rule a built-in texture out.
        is_uncertain: bool,
    },
    /// No texture file anywhere: the game shows its error texture.
    NotFound,
    /// The file exists but cannot be shown.
    Unreadable(PreviewUnreadable),
}

/// Why a texture read could not be answered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadDefTextureError {
    /// The def's graphic could not be resolved.
    #[error(transparent)]
    Resolve(#[from] ResolveDefGraphicError),
    /// `key` is not a texture of this def's resolved graphic.
    #[error("{0} is not a texture of this def's graphic")]
    KeyNotInGraphic(TextureKey),
    /// The owning mod of a loose file is not in the report.
    #[error("{0} is not a scanned mod")]
    UnknownOwner(ModId),
    /// Locating a file failed.
    #[error(transparent)]
    Locate(#[from] DefSourceError),
}

/// Reads one face of a def's resolved graphic.
pub struct ReadDefTexture<Reader, Locator> {
    resolve: ResolveDefGraphic<Reader>,
    locator: Locator,
}

impl<Reader: DefSourceReader, Locator: AssetLocator> ReadDefTexture<Reader, Locator> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(reader: Reader, locator: Locator) -> Self {
        Self {
            resolve: ResolveDefGraphic::new(reader),
            locator,
        }
    }

    /// Reads `key` for `def_ref` under the selected order.
    ///
    /// # Errors
    ///
    /// [`ReadDefTextureError::KeyNotInGraphic`] when the def's resolution
    /// did not produce `key`; the other variants for a failed resolve, an
    /// unscanned owner, or an unreadable folder.
    pub fn execute(
        &self,
        session: &mut Session,
        def_ref: &DefRef,
        key: &TextureKey,
    ) -> Result<DefTexture, ReadDefTextureError> {
        let graphic = self.resolve.execute(session, def_ref)?;
        let Some(face) = find_face(graphic, key) else {
            return Err(ReadDefTextureError::KeyNotInGraphic(key.clone()));
        };
        // The resolution borrows the session mutably; copy out the one fact
        // needed so the report can be read next.
        let availability = face.availability.clone();
        match &availability {
            Availability::Loose { owner } => self.read_loose(session, owner, key),
            Availability::BaseGameOrBundle => Ok(DefTexture::NotViewable {
                is_uncertain: false,
            }),
            Availability::NotFound => Ok(DefTexture::NotFound),
            Availability::Unknown => Ok(DefTexture::NotViewable { is_uncertain: true }),
        }
    }

    fn read_loose(
        &self,
        session: &Session,
        owner: &ModId,
        key: &TextureKey,
    ) -> Result<DefTexture, ReadDefTextureError> {
        let report = session.report();
        let mod_entry = report
            .mods
            .iter()
            .find(|entry| entry.id == *owner)
            .or_else(|| {
                report
                    .mods
                    .iter()
                    .find(|entry| entry.id.base() == owner.base())
            })
            .ok_or_else(|| ReadDefTextureError::UnknownOwner(owner.clone()))?;
        let folders = &mod_entry.loaded_folders;
        let Some(path) = self.locator.locate_texture(folders, key.as_str())? else {
            return Ok(DefTexture::NotFound);
        };
        if !is_dds(&path) {
            return Ok(self.read_image(&path, owner, ImageSource::Direct));
        }
        if is_undecodable(session, owner, key) {
            return Ok(DefTexture::UndecodableInGame {
                owner: owner.clone(),
            });
        }
        match self.locator.locate_non_dds_texture(folders, key.as_str())? {
            Some(sibling) => Ok(self.read_image(&sibling, owner, ImageSource::PngSibling)),
            None => Ok(DefTexture::DdsNotPreviewable {
                owner: owner.clone(),
            }),
        }
    }

    fn read_image(&self, path: &Path, owner: &ModId, from: ImageSource) -> DefTexture {
        match self.locator.read_texture(path) {
            Ok(texture) => DefTexture::Image {
                texture,
                owner: owner.clone(),
                from,
            },
            Err(DefSourceError::TooLarge { .. }) => {
                DefTexture::Unreadable(PreviewUnreadable::TooLarge)
            }
            Err(DefSourceError::UnsupportedFormat { .. }) => {
                DefTexture::Unreadable(PreviewUnreadable::UnsupportedFormat)
            }
            Err(other) => DefTexture::Unreadable(PreviewUnreadable::Io(other.to_string())),
        }
    }
}

/// The face of `graphic` whose key is `key`, if the resolution produced one.
fn find_face<'a>(graphic: &'a DefGraphic, key: &TextureKey) -> Option<&'a Face> {
    let DefGraphic::Resolved(set) = graphic else {
        return None;
    };
    set.slots()
        .iter()
        .flat_map(|slot| slot.variants())
        .flat_map(|variant| variant.faces.all())
        .find(|face| face.key == *key)
}

fn is_dds(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dds"))
}

/// Whether the report says the game cannot decode `owner`'s `.dds` at `key`.
fn is_undecodable(session: &Session, owner: &ModId, key: &TextureKey) -> bool {
    session.report().conflicts.iter().any(|conflict| {
        matches!(
            conflict,
            Conflict::UndecodableTexture(row)
                if row.mod_id.base() == owner.base() && row.path == key.as_str()
        )
    })
}
