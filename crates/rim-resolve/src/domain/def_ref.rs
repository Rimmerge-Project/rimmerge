//! [`DefRef`]: the address of one def or `Name`-attributed template — the
//! inspector's address in URLs, CLI arguments, and finding links
//!

use std::fmt;
use std::str::FromStr;

use rim_analyzer::domain::Selector;
use serde::{Deserialize, Serialize};

use super::finding::DefKey;

/// One def or `Name`-attributed template: [`DefKey`] plus which XML
/// identity it names — a plain `defName` ([`Selector::DefName`]) or a
/// `Name` attribute ([`Selector::NameAttr`], the abstract-template
/// namespace an xpath `[@Name="..."]` predicate matches against, see
/// [`rim_analyzer::extract::xpath_target`]).
///
/// Canonical text form (`Display`/`FromStr`): `<def_type>/<def_name>` for
/// [`Selector::DefName`], `<def_type>/@<def_name>` for
/// [`Selector::NameAttr`] — the `@` mirrors the xpath predicate's own
/// syntax — and a third, type-less form for [`Self::name_only`]: `@<name>`
/// with no `<def_type>/` prefix at all. Persisted the same way
/// [`super::PatchId`]/[`super::FieldPath`] are (`#[serde(try_from =
/// "String", into = "String")]`) rather than as a struct, so a `DefRef` is
/// a plain JSON string everywhere it crosses a boundary — a URL path
/// segment, a CLI argument, a `rimmerge.json`/DTO field.
///
/// Escaping: neither delimiter is escaped. A real RimWorld `defName`/
/// `Name` is a bare identifier-shaped token (letters, digits,
/// `_`/`.`/`-`) that can never contain `/` or start with `@` — but it
/// *can* contain spaces (e.g. `RaidStrategyDef Name="Tribal Siege"`), so a
/// caller crossing a URL must percent-encode the text form and a caller
/// building a CLI argument must quote it; this type does neither for you.
/// Parsing relies on the no-`/`/no-leading-`@` assumption to tell a
/// [`Selector::DefName`] value that happens to start with `@` apart from a
/// [`Selector::NameAttr`] one, and a normal ref apart from the
/// [`Self::name_only`] form — not a universal claim, the same caveat
/// [`super::FindingKey`]'s own proptest doc comment makes about its own
/// grammar: a def type or name that violates it is out of scope for this
/// format, not silently mis-parsed. The def type can never itself contain
/// a `/` (it's whatever precedes the first one); a `/` anywhere in the
/// name portion — a second separator, or one embedded in the name itself
/// — is rejected outright by [`DefRefParseError::EmbeddedSeparator`], and
/// leading or trailing whitespace on any part by
/// [`DefRefParseError::Whitespace`] (never trimmed and accepted: a real
/// name is never surrounded by whitespace, so silently trimming risks
/// matching a mistyped ref to a real one).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DefRef {
    /// The def's type tag and `defName`/`Name`. An empty `def_type` means
    /// [`Self::is_name_only`].
    pub key: DefKey,
    /// Which XML identity `key.def_name` is.
    pub selector: Selector,
}

impl DefRef {
    /// Builds a [`DefRef`] from its parts.
    #[must_use]
    pub fn new(key: DefKey, selector: Selector) -> Self {
        Self { key, selector }
    }

    /// Builds a [`DefRef`] naming just a `Name`-attributed template, with
    /// no known `def_type` — [`crate::domain::FindingKey::DuplicateTemplateName`]'s
    /// own case: `rim_analyzer`'s `Indices::template_owners` is keyed by
    /// `Name` alone, so that finding never has a real `def_type` to put in
    /// an ordinary [`Selector::NameAttr`] ref. Text form `@<name>`, with
    /// no `<def_type>/` prefix — `InspectDef` must resolve this by a
    /// Name-only scan across every def type
    /// (`SourceIndex::children_by_template`'s keys), not a `(def_type,
    /// Name)` lookup.
    #[must_use]
    pub fn name_only(name: impl Into<String>) -> Self {
        Self {
            key: DefKey {
                def_type: String::new(),
                def_name: name.into(),
            },
            selector: Selector::NameAttr,
        }
    }

    /// Whether this is the [`Self::name_only`] form (no `def_type`).
    #[must_use]
    pub fn is_name_only(&self) -> bool {
        self.key.def_type.is_empty() && self.selector == Selector::NameAttr
    }
}

/// [`DefRef`]'s canonical text form failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefRefParseError {
    /// No `/` at all between the def type and the rest.
    #[error("missing '/' separator between def type and name: {0:?}")]
    MissingSeparator(String),
    /// The def type (before the first `/`) was empty.
    #[error("empty def type in {0:?}")]
    EmptyType(String),
    /// The def name (after the `/`, and after a leading `@` if present)
    /// was empty.
    #[error("empty def name in {0:?}")]
    EmptyName(String),
    /// The def name contained a `/` of its own — either a second `/`
    /// separator or one embedded in the name itself.
    #[error("def name contains '/': {0:?}")]
    EmbeddedSeparator(String),
    /// A def type or def name part had leading or trailing whitespace —
    /// never trimmed and accepted, since a real `defName`/`Name`/type tag
    /// is never surrounded by whitespace and silently trimming risks
    /// matching a mistyped ref to a real one.
    #[error("{part} has leading or trailing whitespace: {value:?}")]
    Whitespace {
        /// Which part (`"def type"`, `"def name"`) had the whitespace.
        part: &'static str,
        /// The offending part's own text, untrimmed.
        value: String,
    },
}

impl fmt::Display for DefRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_name_only() {
            return write!(f, "@{}", self.key.def_name);
        }
        match self.selector {
            Selector::DefName => write!(f, "{}/{}", self.key.def_type, self.key.def_name),
            Selector::NameAttr => write!(f, "{}/@{}", self.key.def_type, self.key.def_name),
        }
    }
}

/// Rejects leading or trailing whitespace on one parsed part.
fn reject_whitespace(part: &'static str, value: &str) -> Result<(), DefRefParseError> {
    if value.trim() == value {
        return Ok(());
    }
    Err(DefRefParseError::Whitespace {
        part,
        value: value.to_string(),
    })
}

impl FromStr for DefRef {
    type Err = DefRefParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(name) = text.strip_prefix('@') {
            if name.is_empty() {
                return Err(DefRefParseError::EmptyName(text.to_string()));
            }
            if name.contains('/') {
                return Err(DefRefParseError::EmbeddedSeparator(text.to_string()));
            }
            reject_whitespace("def name", name)?;
            return Ok(Self::name_only(name));
        }

        let (def_type, rest) = text
            .split_once('/')
            .ok_or_else(|| DefRefParseError::MissingSeparator(text.to_string()))?;
        if def_type.is_empty() {
            return Err(DefRefParseError::EmptyType(text.to_string()));
        }
        reject_whitespace("def type", def_type)?;
        let (selector, def_name) = match rest.strip_prefix('@') {
            Some(name_attr) => (Selector::NameAttr, name_attr),
            None => (Selector::DefName, rest),
        };
        if def_name.is_empty() {
            return Err(DefRefParseError::EmptyName(text.to_string()));
        }
        if def_name.contains('/') {
            return Err(DefRefParseError::EmbeddedSeparator(text.to_string()));
        }
        reject_whitespace("def name", def_name)?;
        Ok(Self {
            key: DefKey {
                def_type: def_type.to_string(),
                def_name: def_name.to_string(),
            },
            selector,
        })
    }
}

impl TryFrom<String> for DefRef {
    type Error = DefRefParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<DefRef> for String {
    fn from(value: DefRef) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn key(def_type: &str, def_name: &str) -> DefKey {
        DefKey {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
        }
    }

    #[test]
    fn def_name_selector_renders_without_the_at_sign() {
        let def_ref = DefRef::new(key("ThingDef", "Wall"), Selector::DefName);
        assert_eq!(def_ref.to_string(), "ThingDef/Wall");
    }

    #[test]
    fn name_attr_selector_renders_with_the_at_sign() {
        let def_ref = DefRef::new(key("ThingDef", "WallBase"), Selector::NameAttr);
        assert_eq!(def_ref.to_string(), "ThingDef/@WallBase");
    }

    #[test]
    fn def_name_form_round_trips() {
        let def_ref = DefRef::new(key("ThingDef", "Wall"), Selector::DefName);
        let text = def_ref.to_string();
        assert_eq!(text.parse::<DefRef>().unwrap(), def_ref);
    }

    #[test]
    fn name_attr_form_round_trips() {
        let def_ref = DefRef::new(key("ThingDef", "WallBase"), Selector::NameAttr);
        let text = def_ref.to_string();
        assert_eq!(text.parse::<DefRef>().unwrap(), def_ref);
    }

    #[test]
    fn missing_separator_is_rejected() {
        let result = "ThingDefWall".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::MissingSeparator(_))));
    }

    #[test]
    fn empty_type_is_rejected() {
        let result = "/Wall".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::EmptyType(_))));
    }

    #[test]
    fn empty_def_name_is_rejected() {
        let result = "ThingDef/".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::EmptyName(_))));
    }

    #[test]
    fn empty_name_attr_is_rejected() {
        let result = "ThingDef/@".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::EmptyName(_))));
    }

    #[test]
    fn a_second_slash_in_the_name_is_rejected() {
        let result = "ThingDef/Wall/Extra".parse::<DefRef>();
        assert!(matches!(
            result,
            Err(DefRefParseError::EmbeddedSeparator(_))
        ));
    }

    #[test]
    fn try_from_string_and_into_string_match_display_and_from_str() {
        let def_ref = DefRef::new(key("ThingDef", "Wall"), Selector::DefName);
        let text: String = def_ref.clone().into();
        assert_eq!(text, "ThingDef/Wall");
        assert_eq!(DefRef::try_from(text).unwrap(), def_ref);
    }

    // -- `DefRef::name_only` --

    #[test]
    fn name_only_renders_with_no_def_type_prefix() {
        let def_ref = DefRef::name_only("WallBase");
        assert_eq!(def_ref.to_string(), "@WallBase");
        assert!(def_ref.is_name_only());
    }

    #[test]
    fn name_only_form_round_trips() {
        let def_ref = DefRef::name_only("WallBase");
        let text = def_ref.to_string();
        assert_eq!(text.parse::<DefRef>().unwrap(), def_ref);
    }

    #[test]
    fn an_ordinary_def_ref_is_not_name_only() {
        assert!(!DefRef::new(key("ThingDef", "Wall"), Selector::DefName).is_name_only());
        assert!(!DefRef::new(key("ThingDef", "WallBase"), Selector::NameAttr).is_name_only());
    }

    #[test]
    fn empty_name_only_is_rejected() {
        let result = "@".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::EmptyName(_))));
    }

    #[test]
    fn a_slash_in_a_name_only_ref_is_rejected() {
        let result = "@Wall/Base".parse::<DefRef>();
        assert!(matches!(
            result,
            Err(DefRefParseError::EmbeddedSeparator(_))
        ));
    }

    // -- whitespace rejection (real template names can contain spaces,
    // e.g. `RaidStrategyDef
    // Name="Tribal Siege"`, so only leading/trailing whitespace on a part
    // is rejected, never an interior space) ------------------------------

    #[test]
    fn interior_spaces_in_a_def_name_are_accepted() {
        let def_ref = "RaidStrategyDef/Tribal Siege".parse::<DefRef>().unwrap();
        assert_eq!(def_ref.key.def_name, "Tribal Siege");
    }

    #[test]
    fn leading_whitespace_in_the_def_type_is_rejected() {
        let result = " ThingDef/Wall".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::Whitespace { .. })));
    }

    #[test]
    fn trailing_whitespace_in_the_def_name_is_rejected() {
        let result = "ThingDef/Wall ".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::Whitespace { .. })));
    }

    #[test]
    fn trailing_whitespace_in_a_name_only_ref_is_rejected() {
        let result = "@WallBase ".parse::<DefRef>();
        assert!(matches!(result, Err(DefRefParseError::Whitespace { .. })));
    }

    fn arb_identifier() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9_.-]{0,15}"
    }

    fn arb_def_ref() -> impl Strategy<Value = DefRef> {
        (
            arb_identifier(),
            arb_identifier(),
            prop_oneof![Just(Selector::DefName), Just(Selector::NameAttr)],
        )
            .prop_map(|(def_type, def_name, selector)| {
                DefRef::new(key(&def_type, &def_name), selector)
            })
    }

    proptest! {
        #[test]
        fn def_ref_display_from_str_roundtrips(def_ref in arb_def_ref()) {
            let text = def_ref.to_string();
            let parsed: DefRef = text.parse().unwrap();
            prop_assert_eq!(parsed, def_ref);
        }

        #[test]
        fn name_only_display_from_str_roundtrips(name in arb_identifier()) {
            let def_ref = DefRef::name_only(name);
            let text = def_ref.to_string();
            let parsed: DefRef = text.parse().unwrap();
            prop_assert_eq!(parsed, def_ref);
        }
    }
}
