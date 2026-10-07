//! Sharing a load order: the value objects, the "Copy as text" codec, and
//! the import diff.
//!
//! Everything here is pure. A foreign file format (`.rml`, a
//! `ModsConfig.xml`-shaped list) is parsed by a `rim-io` adapter into a
//! [`SharedModList`]; this module owns what happens before (the text
//! format) and after (the diff against the receiver's install) that.
//!
//! Input is untrusted: every value object is correct by construction, and
//! [`ModListLimits`] bounds what a parser may accept.

mod export;
mod plan;
mod text;
mod values;

pub use export::{ExportListError, ExportedModList};
pub use plan::{
    Activation, CorePlacement, ImportContext, ImportPlan, ImportedEntry, MissingKind, VersionCheck,
    plan_import,
};
pub use text::{parse_text, render_text};
pub use values::{
    InvalidPackageId, InvalidWorkshopId, ListedGameVersion, ListedName, ListedPackageId,
    SharedModEntry, SharedModList, SharedModListError, TruncatedText, WorkshopId,
};

/// The bounds on an imported list. Plain constants so the `rim-io`
/// parser, the CLI's stdin reader and the desktop's paste check all read
/// the same numbers.
#[derive(Debug, Clone, Copy)]
pub struct ModListLimits;

impl ModListLimits {
    /// The most bytes of input any reader may accept (4 MiB). A
    /// 1,000-mod `.rml` carries five lists and is about 200 KB.
    pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
    /// The most entries one list may hold: five times the largest real
    /// list seen.
    pub const MAX_ENTRIES: usize = 5_000;
    /// The longest text-format line, in bytes; a longer line is skipped.
    pub const MAX_LINE_BYTES: usize = 2 * 1024;
    /// roxmltree's `nodes_limit` for an XML import. roxmltree counts
    /// every element and every text node, indentation whitespace
    /// included, so a tab-indented `.rml` costs per entry: 5 lists x
    /// (`li` + its text + the indent after it) = 15 nodes. 16 per entry
    /// leaves room for that, plus 256 for the containers and prolog.
    pub const MAX_XML_NODES: usize = 16 * Self::MAX_ENTRIES + 256;
    /// The longest accepted package id, in characters (`_steam` suffix
    /// included).
    pub const MAX_ID_CHARS: usize = 80;
    /// The longest kept display name, in characters.
    pub const MAX_NAME_CHARS: usize = 200;
    /// The longest kept game-version text, in characters.
    pub const MAX_VERSION_CHARS: usize = 64;
    /// The longest kept excerpt of a malformed entry, in characters.
    pub const MAX_SKIPPED_TEXT_CHARS: usize = 80;
    /// The most [`SkippedEntry`] values a parse reports; the rest are
    /// only counted in [`ParsedModList::omitted_skipped`], so a file of
    /// junk cannot build an unbounded report.
    pub const MAX_SKIPPED_REPORTED: usize = 500;
}

/// A whole-document failure: the input cannot be read as a mod list at
/// all. An outcome of previewing an import, not an error: the caller
/// shows the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The input is longer than `limit_bytes`.
    TooLarge {
        /// [`ModListLimits::MAX_INPUT_BYTES`].
        limit_bytes: usize,
    },
    /// The list has more than `limit` entries.
    TooManyEntries {
        /// [`ModListLimits::MAX_ENTRIES`].
        limit: usize,
    },
    /// The XML is not well formed.
    MalformedXml,
    /// The XML declares a DTD, which a mod list never does.
    DtdNotAllowed,
    /// The XML is nested deeper than any mod list.
    TooDeep,
    /// The input is none of an `.rml`, a `ModsConfig.xml` shape or text.
    UnrecognizedFormat,
    /// An `.rml` without `modList/ids`, or a `ModsConfig.xml` shape
    /// without `activeMods`.
    MissingModList,
    /// The input parsed but named no mod.
    NoEntries,
}

/// One part of the input that was left out of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkippedEntry {
    /// A line of the text format that is not a mod entry.
    NotAnEntry {
        /// 1-based line number.
        line: u32,
    },
    /// An entry whose package id fails the id grammar.
    MalformedId {
        /// 1-based position among the entries of the input.
        position: u32,
        /// A bounded excerpt of the offending text.
        text: TruncatedText,
    },
}

/// A successfully parsed list plus whatever was left out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModList {
    /// The entries that were understood.
    pub list: SharedModList,
    /// What was skipped, at most [`ModListLimits::MAX_SKIPPED_REPORTED`].
    pub skipped: Vec<SkippedEntry>,
    /// How many further skipped parts were not recorded in `skipped`.
    pub omitted_skipped: usize,
}

/// A `usize` count as the `u32` the report types carry. Every count here
/// is bounded by [`ModListLimits::MAX_INPUT_BYTES`], so the saturation is
/// unreachable in practice.
pub(crate) fn count_as_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod test_fixtures;
