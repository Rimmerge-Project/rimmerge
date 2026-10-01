//! [`ModId`]: the canonical, case-insensitive identifier for a mod.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A RimWorld mod `packageId`, normalized to lowercase for comparison.
///
/// RimWorld treats package ids as case-insensitive; every id entering the
/// system is lowercased once at construction so equality and hashing are
/// trivially correct everywhere downstream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModId(String);

impl ModId {
    /// Builds a [`ModId`] from any raw package-id text, trimming whitespace
    /// and lowercasing for canonical comparison.
    pub fn new(raw: impl AsRef<str>) -> Self {
        Self(raw.as_ref().trim().to_lowercase())
    }

    /// Returns the normalized id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Strips the `_steam` suffix RimWorld appends to the workshop copy's
    /// id in `ModsConfig.xml` when a local copy of the same packageId is
    /// also present on disk. `About.xml` fields that name another mod
    /// (`loadAfter`, `modDependencies`, `MayRequire`, `IfModActive`,
    /// `PatchOperationFindMod` package ids) never carry this suffix, and
    /// RimWorld matches them ignoring it — so every comparison between a
    /// declared id and an active id must compare `base()`, not the raw id.
    #[must_use]
    pub fn base(&self) -> Self {
        match self.0.strip_suffix("_steam") {
            Some(stripped) => Self(stripped.to_string()),
            None => self.clone(),
        }
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ModId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ModId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_whitespace() {
        let id = ModId::new("  Ludeon.RimWorld  ");
        assert_eq!(id.as_str(), "ludeon.rimworld");
    }

    #[test]
    fn equal_regardless_of_source_case() {
        assert_eq!(ModId::new("Foo.Bar"), ModId::new("foo.bar"));
    }

    #[test]
    fn base_strips_the_steam_suffix() {
        assert_eq!(ModId::new("foo.bar_steam").base(), ModId::new("foo.bar"));
    }

    #[test]
    fn base_is_identity_when_there_is_no_suffix() {
        assert_eq!(ModId::new("foo.bar").base(), ModId::new("foo.bar"));
    }
}
