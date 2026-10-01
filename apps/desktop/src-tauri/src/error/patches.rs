//! Mapping compatibility-patch errors.

use super::{CommandError, CommandErrorCode};

impl From<rim_session::UnknownPatch> for CommandError {
    fn from(error: rim_session::UnknownPatch) -> Self {
        Self::patch_not_found(error.0.as_str())
    }
}

impl From<rim_resolve::domain::PatchDecisionError> for CommandError {
    fn from(error: rim_resolve::domain::PatchDecisionError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_session::PatchDecideError> for CommandError {
    fn from(error: rim_session::PatchDecideError) -> Self {
        use rim_session::PatchDecideError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Invalid(invalid) => invalid.into(),
        }
    }
}

impl From<rim_resolve::domain::PatchIdParseError> for CommandError {
    fn from(error: rim_resolve::domain::PatchIdParseError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_session::use_cases::CreatePatchError> for CommandError {
    fn from(error: rim_session::use_cases::CreatePatchError) -> Self {
        use rim_session::use_cases::CreatePatchError as E;
        match error {
            E::Identity(e) => Self::patch_identity_invalid(e.to_string()),
            E::Scope(e) => Self::invalid_input(e.to_string()),
            E::InactiveScopeMember(id) => Self::invalid_input(format!("{id} is not an active mod")),
            E::GeneratedScopeMember(id) => Self::invalid_input(format!(
                "{id} is a Rimmerge-generated mod and can't be a patch scope member"
            )),
            E::PackageIdTaken(id) => Self::patch_identity_invalid(format!(
                "package id {id} is already used by an active mod or another patch"
            )),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::UpdatePatchError> for CommandError {
    fn from(error: rim_session::use_cases::UpdatePatchError) -> Self {
        use rim_session::use_cases::UpdatePatchError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Identity(e) => Self::patch_identity_invalid(e.to_string()),
            E::Scope(e) => Self::invalid_input(e.to_string()),
            E::InactiveScopeMember(id) => Self::invalid_input(format!("{id} is not an active mod")),
            E::GeneratedScopeMember(id) => Self::invalid_input(format!(
                "{id} is a Rimmerge-generated mod and can't be a patch scope member"
            )),
            E::PackageIdTaken(id) => Self::patch_identity_invalid(format!(
                "package id {id} is already used by an active mod or another patch"
            )),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::DeletePatchError> for CommandError {
    fn from(error: rim_session::use_cases::DeletePatchError) -> Self {
        use rim_session::use_cases::DeletePatchError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::DecidePatchError> for CommandError {
    fn from(error: rim_session::use_cases::DecidePatchError) -> Self {
        use rim_session::use_cases::DecidePatchError as E;
        match error {
            E::Patch(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::RevertPatchDecisionError> for CommandError {
    fn from(error: rim_session::use_cases::RevertPatchDecisionError) -> Self {
        use rim_session::use_cases::RevertPatchDecisionError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::DecidePatchMergeError> for CommandError {
    fn from(error: rim_session::use_cases::DecidePatchMergeError) -> Self {
        use rim_session::use_cases::DecidePatchMergeError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Plan(e) => e.into(),
            E::UnknownField(path) => Self::invalid_input(format!("unknown field path: {path}")),
            E::UnknownOwner(mod_id) => {
                Self::invalid_input(format!("{mod_id} is not an owner of this finding"))
            }
            E::UnsupportedDrop(path) => Self::invalid_input(format!(
                "{path}: dropping isn't supported for a patch collision"
            )),
            E::Patch(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::ImportProfileDecisionsError> for CommandError {
    fn from(error: rim_session::use_cases::ImportProfileDecisionsError) -> Self {
        use rim_session::use_cases::ImportProfileDecisionsError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::PrunePatchDecisionsError> for CommandError {
    fn from(error: rim_session::use_cases::PrunePatchDecisionsError) -> Self {
        use rim_session::use_cases::PrunePatchDecisionsError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::RenderPatchError> for CommandError {
    fn from(error: rim_session::use_cases::RenderPatchError) -> Self {
        use rim_session::use_cases::RenderPatchError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Render(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::ExportPatchError> for CommandError {
    fn from(error: rim_session::use_cases::ExportPatchError) -> Self {
        use rim_session::use_cases::ExportPatchError as E;
        // The three refusal variants below carry their own descriptive
        // `Display` text (the target path, or "nothing to export") — same
        // idiom as `PlanMergeError`'s mapping above: capture the message
        // before the match consumes `error`.
        let message = error.to_string();
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Render(e) => e.into(),
            E::NothingToExport
            | E::OutDirIsModsFolder(_)
            | E::OutDirNotAbsolute(_)
            | E::ForeignFolder { .. } => Self::invalid_input(message),
            E::Write(e) => Self::new(CommandErrorCode::MergeModIoFailed, e.to_string()),
            E::Store(e) => e.into(),
            E::Config(e) => e.into(),
        }
    }
}
