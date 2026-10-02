//! DTOs: the IPC-boundary shapes every Tauri command speaks, each
//! deriving `serde` (`rename_all = "camelCase"`) and `ts_rs::TS`
//! (`export_to = "../../src/types/generated/"`) so `cargo test` keeps
//! `apps/desktop/src/types/generated/*.ts` in sync with the Rust source
//! of truth. Domain types never cross this boundary directly — every DTO
//! here maps explicitly to and from its `rim-analyzer`/`rim-resolve`/
//! `rim-session` counterpart (see each module for the mapping).
//!
//! [`rim_resolve::domain::FindingKey`] is the one exception living
//! outside this module's structured DTOs: it crosses IPC as its
//! canonical `Display`/`FromStr` text (a plain `String` field named
//! `key` throughout).

pub mod active_set;
pub mod assignment;
pub mod common;
pub mod dashboard;
pub mod def_cache;
pub mod def_conflict;
pub mod def_graphic;
pub mod defs;
pub mod fetch_failure;
pub mod finding;
pub mod game_log;
pub mod game_log_coverage;
pub mod links;
pub mod merge;
pub mod mod_info;
pub mod mods;
pub mod notifications;
pub mod order;
pub mod patch;
pub mod preflight;
pub mod project;
pub mod recommended_rules;
pub mod rule;
pub mod rule_databases;
pub mod settings;
pub mod startup;
pub mod tag;
pub mod texture;
pub mod verify;
