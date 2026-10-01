//! [`TextureKey`]: a validated, normalized texture key.

use std::fmt;

/// Why a raw texture path is not a [`TextureKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TextureKeyError {
    /// The path is empty.
    #[error("texture key is empty")]
    Empty,
    /// The path is longer than [`TextureKey::MAX_BYTES`].
    #[error("texture key is {0} bytes long")]
    TooLong(usize),
    /// The path starts with `/`.
    #[error("texture key starts with a slash")]
    LeadingSlash,
    /// A segment between slashes is empty (`a//b`, a trailing `/`).
    #[error("texture key has an empty segment")]
    EmptySegment,
    /// A segment is `.` or `..`.
    #[error("texture key has a relative segment")]
    RelativeSegment,
    /// The path carries `{` or `[`: a format string or encoded data, never
    /// a file path.
    #[error("texture key looks like a format string")]
    FormatCharacter,
}

/// A texture path in the analyzer's key convention: lowercased,
/// `/`-separated, no extension. Constructed only through
/// [`TextureKey::parse`], so a held key can never climb out of a
/// `Textures/` folder or be a format string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureKey(String);

impl TextureKey {
    /// The longest key accepted, in bytes.
    pub const MAX_BYTES: usize = 256;

    /// Normalizes `raw` (backslashes to slashes, lowercased) and validates it.
    ///
    /// # Errors
    ///
    /// A [`TextureKeyError`] naming the first rule the path breaks.
    pub fn parse(raw: &str) -> Result<Self, TextureKeyError> {
        let key = raw.replace('\\', "/").to_lowercase();
        if key.is_empty() {
            return Err(TextureKeyError::Empty);
        }
        if key.len() > Self::MAX_BYTES {
            return Err(TextureKeyError::TooLong(key.len()));
        }
        if key.starts_with('/') {
            return Err(TextureKeyError::LeadingSlash);
        }
        if key.contains(['{', '[']) {
            return Err(TextureKeyError::FormatCharacter);
        }
        for segment in key.split('/') {
            match segment {
                "" => return Err(TextureKeyError::EmptySegment),
                "." | ".." => return Err(TextureKeyError::RelativeSegment),
                _ => {}
            }
        }
        Ok(Self(key))
    }

    /// The normalized key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TextureKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
