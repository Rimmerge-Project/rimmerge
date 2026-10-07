//! The validated value objects of a shared list.

use std::fmt;
use std::num::NonZeroU64;

use rim_analyzer::domain::ModId;
use thiserror::Error;

use super::ModListLimits;
use super::plan::{CORE_PACKAGE_ID, DLC_PACKAGE_PREFIX};

/// The text is not a Steam Workshop id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("not a Steam Workshop id: 1 to 20 digits, non-zero, fitting 64 bits")]
pub struct InvalidWorkshopId;

/// A Steam Workshop published-file id: never zero (an `.rml` writes `0`
/// for "no Workshop copy").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkshopId(NonZeroU64);

impl WorkshopId {
    /// `None` for `0`.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        NonZeroU64::new(raw).map(Self)
    }

    /// The id as a number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<&str> for WorkshopId {
    type Error = InvalidWorkshopId;

    fn try_from(raw: &str) -> Result<Self, InvalidWorkshopId> {
        let is_digits =
            !raw.is_empty() && raw.len() <= 20 && raw.bytes().all(|b| b.is_ascii_digit());
        if !is_digits {
            return Err(InvalidWorkshopId);
        }
        raw.parse::<u64>()
            .ok()
            .and_then(Self::new)
            .ok_or(InvalidWorkshopId)
    }
}

impl fmt::Display for WorkshopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The text is not an acceptable package id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("not a package id: letters, digits, '.', '_' and '-', with an inner '.'")]
pub struct InvalidPackageId;

/// A package id taken from a list, which is untrusted input. Accepts
/// `[A-Za-z0-9._-]{1,80}` with at least one `.`, not starting or ending
/// with `.`, and with no `..`: the engine's own grammar widened for the
/// `_steam` suffix and for the hyphens and underscores some mods carry.
/// Lowercased like every [`ModId`]. Never used as a path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ListedPackageId(ModId);

impl ListedPackageId {
    /// The id as a [`ModId`].
    #[must_use]
    pub fn as_mod_id(&self) -> &ModId {
        &self.0
    }

    /// The id's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this is Core or one of its DLCs (`ludeon.rimworld`,
    /// `ludeon.rimworld.*`): content the game ships, which a Workshop id
    /// can never name.
    #[must_use]
    pub fn is_official_content(&self) -> bool {
        let base = self.0.base();
        base.as_str() == CORE_PACKAGE_ID || base.as_str().starts_with(DLC_PACKAGE_PREFIX)
    }

    /// The id, consumed.
    #[must_use]
    pub fn into_mod_id(self) -> ModId {
        self.0
    }
}

impl TryFrom<&str> for ListedPackageId {
    type Error = InvalidPackageId;

    fn try_from(raw: &str) -> Result<Self, InvalidPackageId> {
        let raw = raw.trim();
        let is_allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        let is_valid = (1..=ModListLimits::MAX_ID_CHARS).contains(&raw.len())
            && raw.chars().all(is_allowed)
            && raw.contains('.')
            && !raw.starts_with('.')
            && !raw.ends_with('.')
            && !raw.contains("..");
        if is_valid {
            Ok(Self(ModId::new(raw)))
        } else {
            Err(InvalidPackageId)
        }
    }
}

impl fmt::Display for ListedPackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Control characters, plus the zero-width and bidirectional formatting
/// characters that can disguise text (U+200B-200F, U+202A-202E,
/// U+2060-2064, U+2066-2069, U+FEFF).
fn is_stripped(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Removes stripped characters, trims, and keeps at most `max_chars`
/// characters.
fn bounded_text(raw: &str, max_chars: usize) -> String {
    let visible: String = raw.chars().filter(|&c| !is_stripped(c)).collect();
    visible.trim().chars().take(max_chars).collect()
}

macro_rules! bounded_text_type {
    ($(#[$meta:meta])* $name:ident, $limit:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Cleans and bounds `raw`; `None` when nothing is left.
            #[must_use]
            pub fn new(raw: &str) -> Option<Self> {
                let text = bounded_text(raw, $limit);
                (!text.is_empty()).then_some(Self(text))
            }

            /// The cleaned text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

bounded_text_type!(
    /// A mod name taken from a list: at most
    /// [`ModListLimits::MAX_NAME_CHARS`] characters, control characters
    /// removed, never parsed further. Display text only.
    ListedName,
    ModListLimits::MAX_NAME_CHARS
);

bounded_text_type!(
    /// The game version a list was made with, kept as written (at most
    /// [`ModListLimits::MAX_VERSION_CHARS`] characters, control characters
    /// removed). Display text only.
    ListedGameVersion,
    ModListLimits::MAX_VERSION_CHARS
);

/// A bounded excerpt of malformed input, kept so the user can see what
/// was skipped. At most [`ModListLimits::MAX_SKIPPED_TEXT_CHARS`]
/// characters, control characters removed; may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedText(String);

impl TruncatedText {
    /// Cleans and bounds `raw`.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        Self(bounded_text(raw, ModListLimits::MAX_SKIPPED_TEXT_CHARS))
    }

    /// The cleaned text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One mod of a shared list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedModEntry {
    /// The mod's package id.
    pub id: ListedPackageId,
    /// The sender's name for it, if the format carried one.
    pub name: Option<ListedName>,
    /// Its Steam Workshop id, if the format carried one.
    pub workshop_id: Option<WorkshopId>,
}

/// Why a list could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SharedModListError {
    /// A list names at least one mod.
    #[error("a shared list needs at least one mod")]
    Empty,
    /// More entries than [`ModListLimits::MAX_ENTRIES`].
    #[error("a shared list holds at most {limit} mods")]
    TooManyEntries {
        /// [`ModListLimits::MAX_ENTRIES`].
        limit: usize,
    },
}

/// An ordered list of mods someone shared: 1 to
/// [`ModListLimits::MAX_ENTRIES`] entries. Duplicates are kept in order;
/// [`crate::mod_list::plan_import`] reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedModList {
    game_version: Option<ListedGameVersion>,
    entries: Vec<SharedModEntry>,
}

impl SharedModList {
    /// Builds a list, or says why it cannot be one.
    pub fn new(
        game_version: Option<ListedGameVersion>,
        entries: Vec<SharedModEntry>,
    ) -> Result<Self, SharedModListError> {
        if entries.is_empty() {
            return Err(SharedModListError::Empty);
        }
        if entries.len() > ModListLimits::MAX_ENTRIES {
            return Err(SharedModListError::TooManyEntries {
                limit: ModListLimits::MAX_ENTRIES,
            });
        }
        Ok(Self {
            game_version,
            entries,
        })
    }

    /// The game version the list was made with, if known.
    #[must_use]
    pub fn game_version(&self) -> Option<&ListedGameVersion> {
        self.game_version.as_ref()
    }

    /// The entries in list order.
    #[must_use]
    pub fn entries(&self) -> &[SharedModEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_list::test_fixtures::entry;

    #[test]
    fn workshop_id_accepts_digits() {
        let id = WorkshopId::try_from("1234567890").expect("valid");
        assert_eq!(id.get(), 1_234_567_890);
        assert!(WorkshopId::try_from("18446744073709551615").is_ok());
    }

    #[test]
    fn workshop_id_rejects_zero_empty_letters_and_overflow() {
        for raw in [
            "0",
            "",
            "12a",
            "-5",
            " 7",
            "123456789012345678901",
            "18446744073709551616",
        ] {
            assert_eq!(WorkshopId::try_from(raw), Err(InvalidWorkshopId), "{raw:?}");
        }
    }

    #[test]
    fn package_id_accepts_the_grammar_and_lowercases() {
        for raw in [
            "example.framework",
            "example.framework_steam",
            "a-b.c_d",
            "  Example.Framework ",
        ] {
            let id = ListedPackageId::try_from(raw).expect(raw);
            assert_eq!(id.as_str(), raw.trim().to_lowercase());
        }
        let longest = format!("a.{}", "b".repeat(78));
        assert!(ListedPackageId::try_from(longest.as_str()).is_ok());
    }

    #[test]
    fn package_id_rejects_what_the_grammar_forbids() {
        let too_long = format!("a.{}", "b".repeat(79));
        for raw in [
            "nodot", ".lead", "trail.", "a..b", "", "a b.c", "../x", "a/b.c", &too_long,
        ] {
            assert_eq!(
                ListedPackageId::try_from(raw),
                Err(InvalidPackageId),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn official_content_is_core_and_dlc_only() {
        for raw in [
            "ludeon.rimworld",
            "Ludeon.RimWorld.Royalty",
            "ludeon.rimworld.odyssey_steam",
        ] {
            let id = ListedPackageId::try_from(raw).expect(raw);
            assert!(id.is_official_content(), "{raw}");
        }
        for raw in ["ludeon.rimworldx", "example.framework", "ludeon.other"] {
            let id = ListedPackageId::try_from(raw).expect(raw);
            assert!(!id.is_official_content(), "{raw}");
        }
    }

    #[test]
    fn name_strips_control_characters_and_truncates() {
        let long = "x".repeat(300);
        let name = ListedName::new(&long).expect("non-empty");
        assert_eq!(name.as_str().chars().count(), ModListLimits::MAX_NAME_CHARS);

        let cleaned = ListedName::new("  A\u{0}B\nC\u{7f} ").expect("non-empty");
        assert_eq!(cleaned.as_str(), "ABC");
        assert!(ListedName::new(" \n\t").is_none());
    }

    #[test]
    fn name_strips_bidi_and_zero_width_characters() {
        let disguised = "Ex\u{200B}am\u{202E}ple\u{2066}\u{2069}\u{FEFF}\u{200F}";

        let name = ListedName::new(disguised).expect("non-empty");

        assert_eq!(name.as_str(), "Example");
        assert!(ListedName::new("\u{200B}\u{FEFF}").is_none());
    }

    #[test]
    fn truncated_text_is_bounded() {
        let excerpt = TruncatedText::new(&"y".repeat(500));
        assert_eq!(
            excerpt.as_str().chars().count(),
            ModListLimits::MAX_SKIPPED_TEXT_CHARS
        );
    }

    #[test]
    fn list_rejects_zero_and_too_many_entries() {
        assert_eq!(
            SharedModList::new(None, Vec::new()),
            Err(SharedModListError::Empty)
        );

        let at_limit = vec![entry("a.b"); ModListLimits::MAX_ENTRIES];
        assert!(SharedModList::new(None, at_limit).is_ok());

        let over = vec![entry("a.b"); ModListLimits::MAX_ENTRIES + 1];
        assert_eq!(
            SharedModList::new(None, over),
            Err(SharedModListError::TooManyEntries {
                limit: ModListLimits::MAX_ENTRIES
            })
        );
    }
}
