//! Mapping merge planning, texture, merge-mod rendering, and merge-decision errors.

use super::{CommandError, CommandErrorCode};

impl From<rim_session::use_cases::PlanMergeError> for CommandError {
    fn from(error: rim_session::use_cases::PlanMergeError) -> Self {
        use rim_session::use_cases::PlanMergeError as E;
        let message = error.to_string();
        match error {
            // A key that isn't a def-override or patch-collision finding
            // is a caller mistake (a stale/wrong finding key), not a
            // stale scan.
            E::UnsupportedFinding(_) => Self::invalid_input(message),
            E::Source(_) | E::Inherit(_) | E::Xml(_) | E::MissingSource(_) => {
                Self::new(CommandErrorCode::MergeSourceFailed, message)
            }
        }
    }
}

impl From<rim_session::use_cases::ReadTextureError> for CommandError {
    fn from(error: rim_session::use_cases::ReadTextureError) -> Self {
        use rim_session::ports::DefSourceError;
        use rim_session::use_cases::ReadTextureError as E;
        let message = error.to_string();
        match error {
            // An unknown mod or a texture the locator can't find is a
            // caller mistake (a stale finding, a mistyped id), not
            // something a project reload fixes.
            E::UnknownMod(_) | E::TextureNotFound { .. } => Self::invalid_input(message),
            E::Asset(
                DefSourceError::Io { .. }
                | DefSourceError::Stale { .. }
                | DefSourceError::Xml { .. },
            ) => Self::new(CommandErrorCode::MergeSourceFailed, message),
            // Too large is a property of the file itself, not a
            // stale-scan/IO condition — a caller-facing "this isn't a
            // valid request" error, like the mod/texture lookup failures
            // above.
            E::Asset(DefSourceError::TooLarge { .. }) => Self::invalid_input(message),
            // An unsupported format (a `.dds`, for one) gets its own code,
            // so the view renders a translated sentence instead of this
            // English message.
            E::Asset(DefSourceError::UnsupportedFormat { .. }) => {
                Self::new(CommandErrorCode::TextureUnsupportedFormat, message)
            }
        }
    }
}

impl From<rim_session::use_cases::RenderMergeModError> for CommandError {
    fn from(error: rim_session::use_cases::RenderMergeModError) -> Self {
        use rim_session::use_cases::RenderMergeModError as E;
        match error {
            E::Plan(e) => e.into(),
            E::Asset(e) => Self::new(CommandErrorCode::MergeSourceFailed, e.to_string()),
            // See the matching arm on `ApplyError::Emit` above: `render`
            // only ever receives already-`Complete` plans, so this can't
            // actually be reached from `get_merge_mod`/`preview_merge_mod_file`.
            E::Emit(e) => Self::internal(e.to_string()),
        }
    }
}

impl From<rim_session::use_cases::DecideMergeError> for CommandError {
    fn from(error: rim_session::use_cases::DecideMergeError) -> Self {
        use rim_session::use_cases::DecideMergeError as E;
        match error {
            E::Plan(e) => e.into(),
            E::UnknownField(path) => Self::invalid_input(format!("unknown field path: {path}")),
            E::UnknownOwner(mod_id) => {
                Self::invalid_input(format!("{mod_id} is not an owner of this finding"))
            }
            E::UnsupportedDrop(path) => Self::invalid_input(format!(
                "{path}: dropping isn't supported for a patch collision"
            )),
            E::Store(e) => e.into(),
        }
    }
}
