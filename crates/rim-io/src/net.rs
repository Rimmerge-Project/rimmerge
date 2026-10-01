//! Every outbound HTTPS request this crate can ever make goes through this
//! module: [`allowlist`] is the closed host list and the compiled-in
//! [`allowlist::Endpoint`] shape nothing outside this crate can construct
//! a variant of; [`http`] is the [`http::HttpGet`] transport seam
//! ([`http::UreqHttpGet`] the one production implementor, a
//! `#[cfg(test)]`-only fake the other). `databases/cache.rs` and
//! `release_feed.rs` are this module's only two callers.

pub(crate) mod allowlist;
pub(crate) mod http;
