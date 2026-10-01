//! `rim-resolve`: pure domain types for the Rimmerge sorter and
//! resolution ledger — rules, tags, findings, decisions.
//!
//! No filesystem, no XML, no Tauri. Depends only on `rim-analyzer`'s
//! shared-kernel domain types (`ModId`, `Source`, `Report`, ...).

pub mod domain;
pub mod evaluate;
pub mod ledger;
pub mod preflight;
pub mod sort;
pub mod tags;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
