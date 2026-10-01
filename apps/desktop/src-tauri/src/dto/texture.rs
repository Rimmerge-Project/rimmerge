//! DTO for `read_texture`: the texture-override change summary's
//! backend half.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rim_session::ports::TextureFormat;
use rim_session::use_cases::ReadTextureOutput;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Mirrors [`TextureFormat`]. Display-only: never received from the
/// frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TextureFormatDto {
    /// See [`TextureFormat::Png`].
    Png,
    /// See [`TextureFormat::Jpeg`].
    Jpeg,
}

impl From<TextureFormat> for TextureFormatDto {
    fn from(value: TextureFormat) -> Self {
        match value {
            TextureFormat::Png => Self::Png,
            TextureFormat::Jpeg => Self::Jpeg,
        }
    }
}

impl TextureFormatDto {
    /// The MIME image subtype for this format's `data:` URL.
    #[must_use]
    fn mime_subtype(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }
}

/// Base64-encodes `bytes` into a `data:image/<format>;base64,...` URL —
/// shared by [`TextureDto`] and `dto::mod_info::ModPreviewDto`'s own
/// `Image` variant, rather than duplicated between them.
#[must_use]
pub fn encode_data_url(format: TextureFormatDto, bytes: &[u8]) -> String {
    let encoded = BASE64.encode(bytes);
    format!("data:image/{};base64,{encoded}", format.mime_subtype())
}

/// `read_texture`'s response: one mod's texture file, ready to display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TextureDto {
    /// The file's content as a `data:image/<format>;base64,...` URL, ready
    /// for an `<img src>`.
    pub data_url: String,
    /// The sniffed image format.
    pub format: TextureFormatDto,
    /// The file's raw byte count (before base64 encoding).
    pub bytes: usize,
    /// The on-disk path the file was actually read from, for display.
    pub path: String,
}

impl From<&ReadTextureOutput> for TextureDto {
    fn from(value: &ReadTextureOutput) -> Self {
        let format: TextureFormatDto = value.texture.format.into();
        Self {
            data_url: encode_data_url(format, &value.texture.bytes),
            bytes: value.texture.bytes.len(),
            format,
            path: value.path.display().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rim_session::ports::TextureBytes;

    use super::*;

    #[test]
    fn builds_a_data_url_and_counts_raw_bytes() {
        let output = ReadTextureOutput {
            path: PathBuf::from("Mods/A/Textures/things/wall.png"),
            texture: TextureBytes {
                format: TextureFormat::Png,
                bytes: vec![0x89, 0x50, 0x4E, 0x47],
            },
        };

        let dto: TextureDto = (&output).into();

        assert_eq!(dto.format, TextureFormatDto::Png);
        assert_eq!(dto.bytes, 4);
        assert!(dto.data_url.starts_with("data:image/png;base64,"));
        assert_eq!(dto.path, "Mods/A/Textures/things/wall.png");
    }

    #[test]
    fn jpeg_gets_the_jpeg_mime_subtype() {
        let output = ReadTextureOutput {
            path: PathBuf::from("Mods/A/Textures/things/wall.jpg"),
            texture: TextureBytes {
                format: TextureFormat::Jpeg,
                bytes: vec![0xFF, 0xD8, 0xFF],
            },
        };

        let dto: TextureDto = (&output).into();

        assert!(dto.data_url.starts_with("data:image/jpeg;base64,"));
    }
}
