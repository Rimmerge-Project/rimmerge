//! In-memory fakes for every port, plus small fixture builders — for this
//! crate's own use-case tests and for downstream crates' tests
//! (`rim-io`, `apps/cli`). Gated behind the `test-support` feature so it's
//! never compiled into a non-dev build.

mod fakes;
mod projects;
mod scenarios;
mod sessions;

pub use fakes::*;
pub use projects::*;
pub use scenarios::*;
pub use sessions::*;
