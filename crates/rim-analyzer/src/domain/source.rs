//! [`Source`]: where a mod's files come from.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The provenance of a mod's files on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Vanilla RimWorld (`Data/Core`).
    Core,
    /// An official DLC (`Data/<Dlc>`).
    Dlc,
    /// A mod installed locally under `Mods/`.
    Local,
    /// A mod subscribed via the Steam Workshop.
    Workshop,
}

impl Source {
    /// Whether this source is shipped by Ludeon itself. Core/Dlc-owned
    /// assemblies, defs, and textures are never counted as overrides or
    /// undeclared dependencies against other Ludeon content.
    #[must_use]
    pub fn is_vanilla(self) -> bool {
        matches!(self, Self::Core | Self::Dlc)
    }
}

/// Lowercase, matching the wire form (`#[serde(rename_all = "snake_case")]`
/// above) — user-facing text (the analyzer's text summary,
/// `print_inactive_mods`) reads "local"/"workshop", not the `Debug` derive's
/// `Local`/`Workshop`.
impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Core => "core",
            Self::Dlc => "dlc",
            Self::Local => "local",
            Self::Workshop => "workshop",
        };
        f.write_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_core_and_dlc_are_vanilla() {
        assert!(Source::Core.is_vanilla());
        assert!(Source::Dlc.is_vanilla());
        assert!(!Source::Local.is_vanilla());
        assert!(!Source::Workshop.is_vanilla());
    }

    #[test]
    fn display_is_lowercase_and_matches_the_wire_form() {
        assert_eq!(Source::Core.to_string(), "core");
        assert_eq!(Source::Dlc.to_string(), "dlc");
        assert_eq!(Source::Local.to_string(), "local");
        assert_eq!(Source::Workshop.to_string(), "workshop");
    }
}
