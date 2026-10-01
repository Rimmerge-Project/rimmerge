//! [`MergeError`]: [`crate::xml::parse`]'s failure mode — the one place
//! this crate turns arbitrary XML text into a [`crate::tree::FieldTree`],
//! so it owns the one error type malformed input can produce here.
//! [`crate::inherit::InheritError`] and [`crate::emit::EmitError`] are
//! separate, narrower types (each names exactly the failure modes its own
//! module can produce) declared where they're produced.

/// Why [`crate::xml::parse`] failed.
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// The input wasn't well-formed XML at all.
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// An element had element children *and* non-whitespace text —
    /// RimWorld def XML never has mixed content.
    #[error("<{tag}> mixes element children with non-whitespace text")]
    MixedContent {
        /// The offending element's tag.
        tag: String,
    },
    /// The document had no root element to parse as a def/template.
    #[error("document has no root element")]
    EmptyDocument,
    /// The element tree nested deeper than [`crate::xml::MAX_DEPTH`] —
    /// bounding recursion depth against hostile or corrupt input, per
    /// this project's "bound size/depth of anything parsed from untrusted
    /// input" security baseline.
    #[error("element nesting exceeds the {0}-level depth limit")]
    TooDeep(usize),
}
