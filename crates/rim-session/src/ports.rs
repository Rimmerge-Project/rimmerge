//! Ports: the boundaries a [`crate::Session`] and its use cases work
//! through, implemented by infrastructure adapters (`rim-io`) and by
//! [`crate::test_support`]'s in-memory fakes.
//!
//! `load` on [`DecisionStore`] and [`RuleStore`] returns an empty/default
//! value when nothing has been saved yet (a brand-new profile directory)
//! rather than erroring — an error means the file exists but couldn't be
//! read or parsed.

mod app_settings;
mod fetch_failure;
mod game_log;
mod log_coverage;
mod log_shapes;
mod mod_list_file;
mod notifications;
mod process;
mod rules_databases;
mod scan;
mod stores;

pub use app_settings::*;
pub use fetch_failure::*;
pub use game_log::*;
pub use log_coverage::*;
pub use log_shapes::*;
pub use mod_list_file::*;
pub use notifications::*;
pub use process::*;
pub use rules_databases::*;
pub use scan::*;
pub use stores::*;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "ports/ports_tests.rs"]
mod tests;
