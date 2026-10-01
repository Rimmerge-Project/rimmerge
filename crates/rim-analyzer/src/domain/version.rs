//! [`GameVersion`]: a RimWorld `<major>.<minor>` version, used for
//! `ByVersion` merging and `LoadFolders.xml` resolution.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A RimWorld major.minor version, e.g. `1.6`. Patch/revision numbers are
/// deliberately not tracked: every version-sensitive mechanic in RimWorld
/// (`ByVersion` suffixes, `LoadFolders.xml` version folders, on-disk
/// `<major>.<minor>` folders) keys on this pair only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameVersion {
    pub major: u32,
    pub minor: u32,
}

impl GameVersion {
    #[must_use]
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    /// Renders the folder-name form, e.g. `1.6`.
    #[must_use]
    pub fn folder_name(self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// Renders the `ByVersion` XML tag form, e.g. `v1.6`.
    #[must_use]
    pub fn tag_name(self) -> String {
        format!("v{}.{}", self.major, self.minor)
    }
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Error parsing a [`GameVersion`] from text.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot parse game version from {input:?}: {reason}")]
pub struct GameVersionParseError {
    input: String,
    reason: &'static str,
}

impl FromStr for GameVersion {
    type Err = GameVersionParseError;

    /// Parses the first two dot-separated numeric components of `s`,
    /// e.g. `"1.6.4871 rev590"` -> `1.6`, `"1.5"` -> `1.5`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.trim().split('.');
        let err = |reason: &'static str| GameVersionParseError {
            input: s.to_string(),
            reason,
        };
        let major: u32 = parts
            .next()
            .ok_or_else(|| err("missing major component"))?
            .trim()
            .parse()
            .map_err(|_| err("major component is not a number"))?;
        let minor_raw = parts.next().ok_or_else(|| err("missing minor component"))?;
        // The minor component may be trailed by a build number, e.g. "4871"
        // in "1.6.4871"; only the leading digits of the *second* component
        // matter for major.minor, so take digits up to the next non-digit.
        let minor_digits: String = minor_raw.chars().take_while(char::is_ascii_digit).collect();
        let minor: u32 = minor_digits
            .parse()
            .map_err(|_| err("minor component is not a number"))?;
        Ok(Self { major, minor })
    }
}

/// Parses a `v<major>.<minor>` XML tag name (as used by `ByVersion`
/// elements and `LoadFolders.xml`) back into a [`GameVersion`].
pub fn parse_tag_name(tag: &str) -> Option<GameVersion> {
    let stripped = tag.strip_prefix('v')?;
    GameVersion::from_str(stripped).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_major_minor_from_full_version_string() {
        let v: GameVersion = "1.6.4871 rev590".parse().unwrap();
        assert_eq!(v, GameVersion::new(1, 6));
    }

    #[test]
    fn parses_bare_major_minor() {
        let v: GameVersion = "1.5".parse().unwrap();
        assert_eq!(v, GameVersion::new(1, 5));
    }

    #[test]
    fn rejects_missing_minor() {
        let result = "1".parse::<GameVersion>();
        assert!(result.is_err());
    }

    #[test]
    fn parses_version_tag_name() {
        assert_eq!(parse_tag_name("v1.6"), Some(GameVersion::new(1, 6)));
        assert_eq!(parse_tag_name("1.6"), None);
    }

    #[test]
    fn orders_by_major_then_minor() {
        assert!(GameVersion::new(1, 6) > GameVersion::new(1, 5));
        assert!(GameVersion::new(2, 0) > GameVersion::new(1, 9));
    }
}
