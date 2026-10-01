//! Mapping the mod info panel's own "unknown mod" errors
//! (`get_mod_info`/`read_mod_preview`, and `open_mod_link`'s own unknown-mod
//! case, which reuses [`rim_session::mod_info::UnknownMod`]'s mapping
//! below). `open_mod_link`'s other failure — no link of the requested
//! kind exists for this mod — has no domain error type to map here at
//! all; it's an ordinary `CommandError::invalid_input` built inline in
//! `commands/mods.rs`.

use super::CommandError;

impl From<rim_session::mod_info::UnknownMod> for CommandError {
    fn from(error: rim_session::mod_info::UnknownMod) -> Self {
        Self::mod_not_found(error.0.as_str())
    }
}

impl From<rim_session::use_cases::ReadModPreviewError> for CommandError {
    fn from(error: rim_session::use_cases::ReadModPreviewError) -> Self {
        Self::mod_not_found(error.0.as_str())
    }
}
