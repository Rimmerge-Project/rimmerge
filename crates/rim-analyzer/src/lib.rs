//! `rim-analyzer`: a read-only analyzer for RimWorld mod load orders.
//!
//! Scans a game install, the Steam workshop, and the active
//! `ModsConfig.xml`, and reports load-order-relevant facts derived from
//! the mod files themselves — hard dependencies, awareness edges, file
//! collisions, and violations of those relationships by the current load
//! order. Never writes to the game or its config.
//!
//! Layout (hexagonal-lite):
//! - [`domain`]: pure value types (no filesystem, no XML).
//! - [`extract`]: pure functions parsing one file's bytes into domain types.
//! - [`infra`]: filesystem discovery and parallel scanning, producing [`domain::ScannedMod`]s.
//! - [`analysis`]: builds edges, detects conflicts, checks the load order,
//!   produces [`domain::Report`].
//! - [`interface`]: renders a [`domain::Report`] as text; `main.rs` is a thin
//!   CLI shell over this crate.
//!
//! Panic policy: `unwrap`/`expect` are denied outside test code (see
//! `clippy.toml`) — every fallible operation on mod data returns a typed
//! `Result` instead.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod analysis;
pub mod domain;
pub mod extract;
pub mod infra;
pub mod interface;
