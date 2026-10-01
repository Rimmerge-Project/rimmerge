//! [`Ledger`]: every finding for one [`super::order::OrderSource`],
//! resolved or not.

use super::order::OrderSource;
use super::resolution::Resolution;

/// Counts of a [`Ledger`]'s entries by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LedgerStats {
    /// Entries whose suggestion met the confidence threshold.
    pub auto: usize,
    /// Entries still awaiting a decision.
    pub needs_input: usize,
    /// Entries the user has decided.
    pub overridden: usize,
    /// Entries that would already be resolved by the suggested order
    /// (only meaningful for a ledger built against
    /// [`OrderSource::Current`]).
    pub resolved_by_suggested: usize,
    /// `Merge` decisions whose preview is [`super::resolution::MergeState::Complete`].
    /// Always `0` coming out of [`crate::ledger::build`] (which has no XML
    /// to build a preview from); `rim-session` recomputes this after
    /// filling in each entry's [`super::resolution::Resolution::merge`].
    pub merged: usize,
    /// `Merge` decisions whose preview is
    /// [`super::resolution::MergeState::NeedsFieldInput`] or
    /// [`super::resolution::MergeState::CannotMerge`]. Same caveat as
    /// [`Self::merged`].
    pub merge_incomplete: usize,
}

/// Every finding for one [`OrderSource`], with its suggestion, status,
/// and (if any) decision already attached.
#[derive(Debug, Clone, PartialEq)]
pub struct Ledger {
    /// Which load order this ledger was built against.
    pub source: OrderSource,
    /// One entry per finding.
    pub entries: Vec<Resolution>,
    /// Summary counts over `entries`.
    pub stats: LedgerStats,
}
