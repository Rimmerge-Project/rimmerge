//! [`CommandError`]: the one serializable failure shape every Tauri
//! command in this crate returns — a stable [`CommandErrorCode`] the
//! frontend can exhaustively switch on, plus a human-readable `message`.
//! Every domain/application error a use case can produce is mapped into
//! it at this boundary, via a `From` impl per source error type, so each
//! mapping stays explicit rather than falling through a catch-all
//! `Display`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::recommended_rules::UnavailableReasonDto;

mod assignments;
mod def_graphics;
mod findings;
mod merge;
mod mods;
mod patches;
mod project;
mod recommended_rules;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "error/error_tests.rs"]
mod tests;

/// The stable, machine-readable discriminant half of a [`CommandError`].
/// A tagged union on the TypeScript side (not a bare `string`), so the
/// frontend can exhaustively `switch` on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CommandErrorCode {
    /// No project has been loaded into the session yet.
    NoProjectLoaded,
    /// The session's lock was poisoned by an earlier panic; the session
    /// was discarded and the project needs reloading.
    SessionLost,
    /// The background task carrying out the command panicked.
    Internal,
    /// A value received from the frontend failed domain validation.
    InvalidInput,
    /// Scanning or analyzing the install failed.
    ScanFailed,
    /// Reading or writing `ModsConfig.xml` failed.
    ModsConfigIoFailed,
    /// Reading or writing a profile file (`decisions.json`/`rules.json`)
    /// failed.
    ProfileIoFailed,
    /// Importing a RimSort database file failed.
    RimsortImportFailed,
    /// `RimWorldWin64.exe` is running and the caller didn't force the
    /// write.
    RimworldRunning,
    /// The requested mod isn't in the active mod list.
    ModNotFound,
    /// The requested finding isn't currently live.
    FindingNotFound,
    /// A def/template/patch file changed on disk since the scan that
    /// located it, its inheritance chain can't be resolved, or a
    /// `ShipAsset` decision's texture file changed or vanished since the
    /// scan that located it — the editor should prompt to reload the
    /// project.
    MergeSourceFailed,
    /// Writing or removing the generated merge mod's folder failed.
    MergeModIoFailed,
    /// No compat patch project with the requested id is loaded.
    PatchNotFound,
    /// A compat patch's proposed identity (package id/display name)
    /// failed validation, or its package id collides with an active mod
    /// or another patch — the identity form should show the message
    /// inline.
    PatchIdentityInvalid,
    /// The requested def/template ref doesn't name anything this scan
    /// indexed.
    DefNotFound,
    /// No assignment (patch maker) project with the requested id is loaded.
    AssignmentNotFound,
    /// `remove_assignment_section` without `force`: the section's own
    /// free-standing rows are still referenced by another section's
    /// `ItemSlot` value — see [`CommandErrorDetail::AssignmentSectionInUse`]
    /// for the structured list of referencing rows. A dedicated code
    /// not `InvalidInput`: the
    /// frontend handles this one specially (a "still referenced" dialog
    /// offering `force`), the same way `RimworldRunning` gets special
    /// handling rather than a generic message, and `InvalidInput` alone
    /// would give it nothing to switch on.
    AssignmentSectionInUse,
    /// `apply` was asked to write `ModsConfig.xml` while the working
    /// active-mod set has pending changes no rescan has picked up yet
    /// — a dedicated code, not
    /// `InvalidInput`: the frontend offers a Rescan action
    /// rather than a plain error message, the same way `RimworldRunning`
    /// gets special handling.
    StaleActiveSet,
    /// A texture file's content is not a format the viewer can show (a
    /// `.dds`, for one): a property of the file, not of the request — the
    /// view says so in its own words instead of echoing this message.
    TextureUnsupportedFormat,
    /// A "Get the recommended rules" run is already in progress in this
    /// process (a double click, or a second window); the caller waits for
    /// the first run instead of starting another.
    AlreadyRunning,
    /// "Get the recommended rules" was asked to download while a network
    /// gate is closed (the offline switch, or the first-run notice not yet
    /// answered): [`CommandErrorDetail::RecommendedRulesUnavailable`] says
    /// which.
    RecommendedRulesUnavailable,
    /// `app-settings.json` is damaged (unreadable or corrupt) and was left
    /// untouched: the fix is Settings' repair, not a retry, so it is not
    /// `ProfileIoFailed`.
    AppSettingsDamaged,
}

/// Structured detail some [`CommandError`]s carry alongside their plain
/// `message`, for a code whose failure has more shape than prose can
/// render on its own — a renderable payload (a list, e.g.) rather than
/// text the frontend would have to re-parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CommandErrorDetail {
    /// See [`CommandErrorCode::AssignmentSectionInUse`].
    AssignmentSectionInUse {
        /// Every row, in another section, still naming one of the
        /// removed section's own free-standing rows.
        ///
        /// `#[serde(rename = "referencedBy")]`: `#[serde(tag = "kind",
        /// rename_all = "camelCase")]` on the enum only renames *variant*
        /// names (and the tag's own value) — it does not touch a struct
        /// variant's own field names, so without this the field shipped
        /// as `referenced_by` on the wire despite every other DTO in this
        /// crate being camelCase (`apps/desktop/CLAUDE.md` warns about
        /// exactly this).
        #[serde(rename = "referencedBy")]
        referenced_by: Vec<SectionReferenceDto>,
    },
    /// See [`CommandErrorCode::RecommendedRulesUnavailable`].
    RecommendedRulesUnavailable {
        /// Which network gate is closed.
        reason: UnavailableReasonDto,
    },
}

/// One row, in another section, that still names a section's own
/// free-standing row — [`CommandErrorDetail::AssignmentSectionInUse`]'s
/// own per-row shape, built from
/// [`rim_resolve::domain::SectionError::SectionInUse`]'s
/// `referenced_by: Vec<(String, RowKey, FieldPath)>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SectionReferenceDto {
    /// The referencing section's own def type.
    pub def_type: String,
    /// The referencing row's own key: a target-keyed row's target def
    /// (`ThingDef/Race0`), a free-standing row's own bare `defName` — the
    /// same human-readable shape `apps/cli`'s own `format_row_key` builds
    /// (never a domain `Debug`/bare `Display`, this interface's own
    /// convention for a user-facing row address).
    pub row: String,
    /// The field, by canonical path text, whose value names the
    /// now-removed section's row.
    pub path: String,
}

/// A [`RowKey`](rim_resolve::domain::RowKey) the way a human reads it —
/// mirrors `apps/cli`'s own `format_row_key`. `pub(crate)`: also used by
/// `dto::assignment`'s own `StrandedValueDto` mapping, which needs the
/// identical rendering for an [`UpdateAssignmentOutcome::stranded_values`](rim_session::use_cases::UpdateAssignmentOutcome::stranded_values) row key.
pub(crate) fn format_row_key(key: &rim_resolve::domain::RowKey) -> String {
    match key {
        rim_resolve::domain::RowKey::Target(target) => target.def.to_string(),
        rim_resolve::domain::RowKey::Own(name) => name.clone(),
    }
}

/// A command failure, serialized to the frontend as `{ code, message,
/// detail }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CommandError {
    /// A short, stable discriminant the frontend can match on.
    pub code: CommandErrorCode,
    /// A human-readable description, safe to display as-is.
    pub message: String,
    /// Structured detail, present only for the codes that carry one — see
    /// [`CommandErrorDetail`].
    #[serde(default)]
    #[ts(optional = nullable)]
    pub detail: Option<CommandErrorDetail>,
}

impl CommandError {
    /// Builds an error from an explicit code and message, with no
    /// structured [`CommandErrorDetail`].
    #[must_use]
    pub fn new(code: CommandErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    /// Builds an error carrying structured [`CommandErrorDetail`]
    /// alongside its message — used only by mappings whose domain error
    /// needs more than prose to render (currently just
    /// [`CommandErrorCode::AssignmentSectionInUse`]).
    #[must_use]
    pub fn with_detail(
        code: CommandErrorCode,
        message: impl Into<String>,
        detail: CommandErrorDetail,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Some(detail),
        }
    }

    /// No project has been loaded into the session yet.
    #[must_use]
    pub fn no_project_loaded() -> Self {
        Self::new(CommandErrorCode::NoProjectLoaded, "no project is loaded")
    }

    /// A command panicked while mutating the session (or, defensively,
    /// found the lock already poisoned by something else — see
    /// `state::with_session`'s own doc comment); the caller already
    /// discarded the session, and the project needs reloading.
    #[must_use]
    pub fn session_lost() -> Self {
        Self::new(
            CommandErrorCode::SessionLost,
            "the session was lost after an earlier internal error; reload the project",
        )
    }

    /// The background task carrying out the command panicked somewhere
    /// outside `state::with_session`'s own `catch_unwind` (a panic
    /// inside it is `session_lost` instead — see that function's doc
    /// comment), or some other internal failure with no more specific
    /// code.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(CommandErrorCode::Internal, message.into())
    }

    /// A value received from the frontend failed domain validation (an
    /// invalid tag slug, cluster rule id, edge kind, or finding key).
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(CommandErrorCode::InvalidInput, message.into())
    }

    /// The requested mod isn't in the active mod list.
    #[must_use]
    pub fn mod_not_found(mod_id: &str) -> Self {
        Self::new(
            CommandErrorCode::ModNotFound,
            format!("{mod_id} is not active"),
        )
    }

    /// The requested finding isn't currently live.
    #[must_use]
    pub fn finding_not_found(key: &str) -> Self {
        Self::new(
            CommandErrorCode::FindingNotFound,
            format!("{key} is not a live finding"),
        )
    }

    /// `RimWorldWin64.exe` is running and the caller didn't force the
    /// write.
    #[must_use]
    pub fn rimworld_running() -> Self {
        Self::new(
            CommandErrorCode::RimworldRunning,
            "RimWorldWin64.exe is running; close the game or retry with force",
        )
    }

    /// No compat patch project with `id` is loaded.
    #[must_use]
    pub fn patch_not_found(id: &str) -> Self {
        Self::new(
            CommandErrorCode::PatchNotFound,
            format!("no patch project with id {id}"),
        )
    }

    /// A compat patch's proposed identity is invalid, or its package id
    /// collides with an active mod or another patch.
    #[must_use]
    pub fn patch_identity_invalid(message: impl Into<String>) -> Self {
        Self::new(CommandErrorCode::PatchIdentityInvalid, message.into())
    }

    /// `def_ref` doesn't name anything this scan indexed as a def or
    /// template.
    #[must_use]
    pub fn def_not_found(def_ref: &str) -> Self {
        Self::new(
            CommandErrorCode::DefNotFound,
            format!("{def_ref} is not indexed as a def or template"),
        )
    }

    /// A "Get the recommended rules" run is already in progress.
    #[must_use]
    pub fn already_running() -> Self {
        Self::new(
            CommandErrorCode::AlreadyRunning,
            "the recommended rules are already being downloaded",
        )
    }

    /// No assignment (patch maker) project with `id` is loaded.
    #[must_use]
    pub fn assignment_not_found(id: &str) -> Self {
        Self::new(
            CommandErrorCode::AssignmentNotFound,
            format!("no assignment project with id {id}"),
        )
    }
}
