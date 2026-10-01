//! DTOs for the inbox: findings, suggestions, decisions, and the actions
//! that resolve them.
//!
//! [`ActionDto`] is the one structured (non-string) domain object beside
//! DTOs proper that crosses IPC in both directions here — `decide` needs
//! a real [`rim_resolve::domain::Action`] to hand `Session::decide`, and
//! the inbox needs to display a suggestion's/alternative's action back —
//! so it gets full round-trip `From`/`TryFrom` mappings, unlike the
//! display-only [`FindingDto`].

mod evidence;
mod key;
mod rationale;
mod suggestion;

pub use evidence::*;
pub use key::*;
pub use rationale::*;
pub use suggestion::*;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "finding/finding_tests.rs"]
mod tests;
