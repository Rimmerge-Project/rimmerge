//! Builds a [`crate::domain::Ledger`] for one
//! [`crate::domain::OrderSource`] from a [`rim_analyzer::domain::Report`],
//! the sorter's [`crate::sort::SortOutcome`], a
//! [`crate::domain::DecisionSet`], and a confidence threshold.
//!
//! `findings` extracts every live finding; `suggest` is the table-driven
//! confidence function; `build`
//! assembles the two (plus decisions) into one [`crate::domain::Ledger`].

mod build;
mod findings;
mod scoped;
mod suggest;

pub use build::{BuildLedgerInput, build};
pub use findings::extract as extract_findings;
pub use scoped::{ScopedLedgerInput, scoped};
pub use suggest::{DefOverrideDirection, SuggestContext, def_override_direction, suggest};

// Re-exported so the `rim_resolve::ledger::{ResolveError, validate_action}`
// path keeps resolving: both live in `domain` (see `domain::decision`), since
// `DecisionSet::insert` — a domain type's own method — enforces this
// validation directly, and `domain` never imports from `ledger`.
pub use crate::domain::{ResolveError, validate_action};
