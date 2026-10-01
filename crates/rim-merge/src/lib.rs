//! `rim-merge`: turns raw def XML into a field-addressable tree, resolves
//! `ParentName` inheritance, three-way diffs contested owners, replays
//! patch operations for contested collisions, folds a user's per-field
//! choices into a patch plan, and emits the generated merge mod's files.
//!
//! Pure: no filesystem, no Tauri. The session hands this crate XML text
//! it already read through its own `DefSourceReader` port; this crate
//! never reads a file itself.
//!
//! Layout: [`tree`] (the field model + addressing), [`xml`] (XML text <->
//! [`tree::FieldTree`]), [`inherit`] (`ParentName` chain resolution),
//! [`diff`] (three-way per-field diff), [`patch_behaviours`] (the
//! data-driven custom-class/gate table the replay consults),
//! [`patch_eval`] (the xpath-subset
//! replay evaluator), [`plan`] ((diff, choices) -> a patch plan), [`emit`]
//! (plan -> the generated mod's files), [`effective`] (the game's own
//! pipeline over one contested def, attributed field-by-field, with no
//! choices involved), [`assign`] (the generic assignment-def
//! inference and `Defs/` emission engine), [`error`] (crate-wide error
//! type).

#![forbid(unsafe_code)]

pub mod assign;
pub mod diff;
pub mod effective;
pub mod emit;
pub mod error;
pub mod inherit;
pub mod patch_behaviours;
pub mod patch_eval;
pub mod plan;
pub mod tree;
pub mod xml;
