//! The validated value objects of the organization model: ids and names.

use std::fmt;
use std::num::NonZeroU32;

use rim_analyzer::domain::ModId;
use thiserror::Error;

use super::OrganizationLimits;

/// The id cannot name an organized mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidOrganizedModId {
    /// The id is empty once trimmed and stripped of `_steam`.
    #[error("a mod id cannot be empty")]
    Empty,
    /// The id is longer than [`OrganizationLimits::MAX_ID_CHARS`].
    #[error("a mod id is at most {limit} characters")]
    TooLong {
        /// [`OrganizationLimits::MAX_ID_CHARS`].
        limit: usize,
    },
    /// The id holds a control character.
    #[error("a mod id cannot hold control characters")]
    ControlCharacter,
}

/// A mod as organization knows it: always a **base** id (`_steam` stripped,
/// lowercased like every [`ModId`]), so the Workshop and the local copy of
/// one package are one member. Not a path, never used as one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrganizedModId(ModId);

impl OrganizedModId {
    /// The organized identity of `id`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidOrganizedModId`] for an empty, overlong or
    /// control-character id: a file the aggregate wrote must always be
    /// readable back, so the bounds the reader enforces apply here too.
    pub fn new(id: &ModId) -> Result<Self, InvalidOrganizedModId> {
        let base = id.base();
        let text = base.as_str();
        if text.is_empty() {
            return Err(InvalidOrganizedModId::Empty);
        }
        if text.chars().count() > OrganizationLimits::MAX_ID_CHARS {
            return Err(InvalidOrganizedModId::TooLong {
                limit: OrganizationLimits::MAX_ID_CHARS,
            });
        }
        if text.chars().any(char::is_control) {
            return Err(InvalidOrganizedModId::ControlCharacter);
        }
        Ok(Self(base))
    }

    /// The base id as a [`ModId`].
    #[must_use]
    pub fn as_mod_id(&self) -> &ModId {
        &self.0
    }

    /// The base id's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for OrganizedModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A group's identity inside one [`super::ModOrganization`]: never zero,
/// allocated by the aggregate, never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(NonZeroU32);

impl GroupId {
    /// `None` for `0`.
    #[must_use]
    pub fn new(raw: u32) -> Option<Self> {
        NonZeroU32::new(raw).map(Self)
    }

    /// The id as a number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The text is not an acceptable group name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidGroupName {
    /// Empty once trimmed.
    #[error("a group name cannot be empty")]
    Empty,
    /// Longer than [`OrganizationLimits::MAX_NAME_CHARS`].
    #[error("a group name is at most {limit} characters")]
    TooLong {
        /// [`OrganizationLimits::MAX_NAME_CHARS`].
        limit: usize,
    },
    /// Holds a control character (a newline included).
    #[error("a group name cannot hold control characters")]
    ControlCharacter,
}

/// A group's display name: trimmed, 1 to
/// [`OrganizationLimits::MAX_NAME_CHARS`] characters, no control
/// characters, Unicode kept as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GroupName(String);

impl GroupName {
    /// Validates `raw`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidGroupName`] when `raw` is empty, too long or holds
    /// a control character.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, InvalidGroupName> {
        let text = raw.as_ref().trim();
        if text.is_empty() {
            return Err(InvalidGroupName::Empty);
        }
        if text.chars().count() > OrganizationLimits::MAX_NAME_CHARS {
            return Err(InvalidGroupName::TooLong {
                limit: OrganizationLimits::MAX_NAME_CHARS,
            });
        }
        if text.chars().any(char::is_control) {
            return Err(InvalidGroupName::ControlCharacter);
        }
        Ok(Self(text.to_owned()))
    }

    /// The name as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The key two sibling names are compared by (case-insensitive, no
    /// locale collation): the name Unicode-lowercased.
    #[must_use]
    pub fn uniqueness_key(&self) -> String {
        self.0.to_lowercase()
    }

    /// The display order of two names: lowercased first (no locale
    /// collation, so it never depends on the machine), then as written.
    /// Callers break a remaining tie on the [`GroupId`].
    #[must_use]
    pub fn display_order(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .to_lowercase()
            .cmp(&other.0.to_lowercase())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl fmt::Display for GroupName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
