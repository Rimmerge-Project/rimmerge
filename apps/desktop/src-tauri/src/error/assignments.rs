//! Mapping patch-maker (assignment) errors.

use std::fmt::Write as _;

use super::{
    CommandError, CommandErrorCode, CommandErrorDetail, SectionReferenceDto, format_row_key,
};

impl From<rim_session::UnknownAssignment> for CommandError {
    fn from(error: rim_session::UnknownAssignment) -> Self {
        Self::assignment_not_found(error.0.as_str())
    }
}

impl From<rim_resolve::domain::AssignmentIdParseError> for CommandError {
    fn from(error: rim_resolve::domain::AssignmentIdParseError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_resolve::domain::AssignmentRowError> for CommandError {
    fn from(error: rim_resolve::domain::AssignmentRowError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

/// Maps a section-level domain refusal. Section-not-found/ambiguous
/// addressing never reaches here: `commands::assignments::resolve_section`/
/// `resolve_section_type` build those as plain `CommandError::invalid_input`
/// errors themselves.
impl From<rim_resolve::domain::SectionError> for CommandError {
    fn from(error: rim_resolve::domain::SectionError) -> Self {
        use rim_resolve::domain::SectionError as E;
        match error {
            E::AlreadyExists(def_type) => Self::invalid_input(format!(
                "a section for def type {def_type:?} already exists"
            )),
            E::SectionInUse { referenced_by } => {
                let referenced_by: Vec<SectionReferenceDto> = referenced_by
                    .iter()
                    .map(|(def_type, key, path)| SectionReferenceDto {
                        def_type: def_type.clone(),
                        row: format_row_key(key),
                        path: path.to_string(),
                    })
                    .collect();
                let mut message = format!(
                    "section is still referenced by {} row(s) in other sections:",
                    referenced_by.len()
                );
                for reference in &referenced_by {
                    let _ = write!(
                        message,
                        "\n  {} {} {}",
                        reference.def_type, reference.row, reference.path
                    );
                }
                message.push_str(
                    "\npass force to remove it anyway (references are left dangling; \
                     export reports them as skips)",
                );
                Self::with_detail(
                    CommandErrorCode::AssignmentSectionInUse,
                    message,
                    CommandErrorDetail::AssignmentSectionInUse { referenced_by },
                )
            }
        }
    }
}

impl From<rim_session::use_cases::AddAssignmentSectionError> for CommandError {
    fn from(error: rim_session::use_cases::AddAssignmentSectionError) -> Self {
        use rim_session::use_cases::AddAssignmentSectionError as E;
        let message = error.to_string();
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Propose(e) => e.into(),
            E::NotOwnedByRefs(_) | E::NoValidCandidate(_) => Self::invalid_input(message),
            E::Section(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::RemoveAssignmentSectionError> for CommandError {
    fn from(error: rim_session::use_cases::RemoveAssignmentSectionError) -> Self {
        use rim_session::use_cases::RemoveAssignmentSectionError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Section(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::CopyFromError> for CommandError {
    fn from(error: rim_session::use_cases::CopyFromError) -> Self {
        match error {
            rim_session::use_cases::CopyFromError::UnknownSection(def_type) => Self::invalid_input(
                format!("this project has no section for def type {def_type:?}"),
            ),
        }
    }
}

impl From<rim_session::use_cases::AssignmentInstancesError> for CommandError {
    fn from(error: rim_session::use_cases::AssignmentInstancesError) -> Self {
        use rim_session::use_cases::AssignmentInstancesError as E;
        let message = error.to_string();
        match error {
            E::Source(_) | E::Xml(_) => Self::new(CommandErrorCode::MergeSourceFailed, message),
            // A stale/inconsistent scan naming an owner the index itself
            // has no record of — unreachable in ordinary use (see that
            // variant's own doc comment), `internal` names that rather
            // than inventing a dedicated code for a path that can't
            // actually be hit (mirrors `RenderMergeModError::Emit`'s own
            // rationale).
            E::MissingSource(_) => Self::internal(message),
        }
    }
}

impl From<rim_session::use_cases::ProposeAssignmentError> for CommandError {
    fn from(error: rim_session::use_cases::ProposeAssignmentError) -> Self {
        use rim_session::use_cases::ProposeAssignmentError as E;
        match error {
            E::Instances(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::CreateAssignmentError> for CommandError {
    fn from(error: rim_session::use_cases::CreateAssignmentError) -> Self {
        use rim_session::use_cases::CreateAssignmentError as E;
        let message = error.to_string();
        match error {
            E::Identity(e) => Self::patch_identity_invalid(e.to_string()),
            E::EmptyRefs | E::EmptyTargets => Self::invalid_input(message),
            E::PackageIdTaken(id) => Self::patch_identity_invalid(format!(
                "package id {id} is already used by an active mod or another project"
            )),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::UpdateAssignmentError> for CommandError {
    fn from(error: rim_session::use_cases::UpdateAssignmentError) -> Self {
        use rim_session::use_cases::UpdateAssignmentError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Instances(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::SetAssignmentRowError> for CommandError {
    fn from(error: rim_session::use_cases::SetAssignmentRowError) -> Self {
        use rim_session::use_cases::SetAssignmentRowError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Row(e) => e.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::ClearAssignmentRowError> for CommandError {
    fn from(error: rim_session::use_cases::ClearAssignmentRowError) -> Self {
        use rim_session::use_cases::ClearAssignmentRowError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::DeleteAssignmentError> for CommandError {
    fn from(error: rim_session::use_cases::DeleteAssignmentError) -> Self {
        use rim_session::use_cases::DeleteAssignmentError as E;
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Store(e) => e.into(),
        }
    }
}

impl From<rim_session::use_cases::AssignmentCoverageError> for CommandError {
    fn from(error: rim_session::use_cases::AssignmentCoverageError) -> Self {
        use rim_session::use_cases::AssignmentCoverageError as E;
        let message = error.to_string();
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::Source(e) => e.into(),
            E::Inherit(_) => Self::new(CommandErrorCode::MergeSourceFailed, message),
        }
    }
}

impl From<rim_session::use_cases::ExportAssignmentError> for CommandError {
    fn from(error: rim_session::use_cases::ExportAssignmentError) -> Self {
        use rim_session::use_cases::ExportAssignmentError as E;
        // Mirrors `ExportPatchError`'s own mapping exactly (see that impl
        // above): the refusal variants carry their own descriptive
        // `Display` text, captured before the match consumes `error`.
        let message = error.to_string();
        match error {
            E::Unknown(unknown) => unknown.into(),
            E::NothingToExport
            | E::OutDirIsModsFolder(_)
            | E::OutDirNotAbsolute(_)
            | E::ForeignFolder { .. } => Self::invalid_input(message),
            E::Write(e) => Self::new(CommandErrorCode::MergeModIoFailed, e.to_string()),
            // `ExportAssignment` renders directly rather than through
            // `RenderMergeMod` (see that use case's own doc comment), but
            // `emit::render` failing is just as structurally unreachable
            // here as `RenderMergeModError::Emit`/`ApplyError::Emit` are
            // for their own always-already-valid inputs.
            E::Emit(e) => Self::internal(e.to_string()),
            E::Store(e) => e.into(),
            E::Config(e) => e.into(),
        }
    }
}
