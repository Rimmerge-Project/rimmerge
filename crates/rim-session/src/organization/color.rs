//! The fixed palette a mod or a group can be coloured with.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// The text names no palette colour.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a palette colour: {0:?}")]
pub struct UnknownColor(pub String);

/// One of the twelve named colours. A palette, not a hex value: the
/// interface maps each token to a light/dark pair checked for contrast,
/// and the name is the colour's accessible text. "No colour" is the
/// absence of a token (`Option<ColorToken>`), never a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorToken {
    /// Red.
    Red,
    /// Orange.
    Orange,
    /// Amber.
    Amber,
    /// Lime.
    Lime,
    /// Green.
    Green,
    /// Teal.
    Teal,
    /// Cyan.
    Cyan,
    /// Blue.
    Blue,
    /// Indigo.
    Indigo,
    /// Violet.
    Violet,
    /// Pink.
    Pink,
    /// Slate.
    Slate,
}

impl ColorToken {
    /// Every token, in palette order.
    pub const ALL: [Self; 12] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Lime,
        Self::Green,
        Self::Teal,
        Self::Cyan,
        Self::Blue,
        Self::Indigo,
        Self::Violet,
        Self::Pink,
        Self::Slate,
    ];

    /// The token's snake_case wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Amber => "amber",
            Self::Lime => "lime",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
            Self::Indigo => "indigo",
            Self::Violet => "violet",
            Self::Pink => "pink",
            Self::Slate => "slate",
        }
    }
}

impl FromStr for ColorToken {
    type Err = UnknownColor;

    fn from_str(text: &str) -> Result<Self, UnknownColor> {
        Self::ALL
            .into_iter()
            .find(|token| token.as_str() == text)
            .ok_or_else(|| UnknownColor(text.to_owned()))
    }
}

impl fmt::Display for ColorToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
