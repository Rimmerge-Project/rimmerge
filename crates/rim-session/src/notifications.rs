//! Notices about the app itself — its network policy, caches, and its own
//! version — never about the sort or the ledger (`rim-resolve` owns
//! those). [`version`] holds the value objects a version check compares;
//! [`model`] holds the notice model itself; [`evaluate`] holds the pure
//! function that turns current state into the active list. The
//! state/port types (`NotificationStateStore`, `NotificationState`) live
//! in `crate::ports::notifications` instead, alongside `ReleaseFeed`, the
//! same split every other port/domain pair in this crate follows.

pub(crate) mod clock;
pub mod evaluate;
pub mod model;
pub mod version;

// `evaluate` (the function) is deliberately **not** re-exported here
// alongside the `evaluate` module — rustdoc treats a module and a
// same-named re-exported item at the same scope as ambiguous
// ("`evaluate` is both a function and a module"). Reach it as
// `crate::notifications::evaluate::evaluate`, or `use
// crate::notifications::evaluate::evaluate;` at the call site (every
// current caller already does the latter).
pub use evaluate::NotificationInputs;
pub use model::{
    Dismissal, Freshness, Notification, NotificationAction, NotificationKey, NotificationKind,
    RefreshMode, Severity, SourceSetup, StaleSource, imported_rules_outdated_fingerprint,
};
pub use version::{
    AppVersion, AppVersionError, GameMajorMinor, GameMajorMinorParseError, LatestRelease,
};
