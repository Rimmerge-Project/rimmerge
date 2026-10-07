//! Subcommand implementations. Thin: parse args (via `clap`), delegate to
//! `rim-resolve`/`rim-session`/`rim-io`, print the result. No business
//! logic lives here -- with one established exception, `fixture` (`trim`
//! and `gen`): dev/test fixture tooling that writes files rather than
//! touching a real install or a live session, so it carries its own
//! generation logic directly (`fixture_gen.rs`'s own module doc has the
//! precedent this follows, already set by `fixture trim`/
//! `rim-resolve/examples/trim_fixture.rs`).

pub mod apply;
pub mod assign;
pub mod check_update;
pub mod config;
pub mod db;
pub mod defs;
pub mod fixture;
pub mod fixture_gen;
pub mod import;
pub mod ledger;
pub mod load;
pub mod log;
pub mod merge;
pub mod mods;
pub mod network;
pub mod order;
pub mod patch;
pub mod promote;
pub mod rule;
pub mod sort;
pub mod startup;
pub mod verify;
