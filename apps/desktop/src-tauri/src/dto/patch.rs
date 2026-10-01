//! DTOs for the compat-patch commands: `list_patches`, `get_patch`, `create_patch`, `update_patch`,
//! `delete_patch`, `list_patch_findings`, `get_patch_finding`,
//! `decide_patch`, `revert_patch_decision`, `import_profile_decisions`,
//! `prune_patch_decisions`, `get_patch_render`, `preview_patch_file`,
//! `export_patch`. `list_patch_findings`/`get_patch_finding` reuse
//! [`super::finding::FindingPageDto`]/[`super::finding::ResolutionSummaryDto`]/
//! [`super::finding::ResolutionDetailDto`] as-is — a patch's scoped ledger
//! produces the same [`rim_resolve::domain::Resolution`] shape the profile
//! ledger does, distinguished only by [`super::finding::ResolutionSummaryDto::scope`]/
//! [`super::finding::ResolutionDetailDto::scope`] being `Some`.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    FindingKey, LedgerStats, PatchDecisionError, PatchProject, PatchScope, ScopeChange,
    ScopeMembership, UnpatchableAction,
};
use rim_session::use_cases::{CreatePatchInput, ImportReport, MergeModRender, UpdatePatchInput};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::finding::{ActionDto, FindingFilterDto, LedgerStatsDto};
use super::merge::{CaveatDto, MergeModEntryDto, MergeSourceModDto};

/// One mod named by id and display name — a scope member, or a merge
/// mod's own declared dependency. Mirrors the `(ModId, display name)`
/// pairs this crate's other DTOs (e.g. [`super::merge::MergeSourceModDto`])
/// already carry individually; kept as its own named type so every mod
/// list shares one shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModRefDto {
    /// The mod's id.
    pub mod_id: String,
    /// The mod's display name.
    pub name: String,
}

/// Whether a finding is fully or only partly inside a patch's scope.
/// Mirrors [`ScopeMembership`] — [`ScopeMembership::Outside`] has no
/// variant here: a scoped ledger ([`rim_resolve::ledger::scoped`]) never
/// keeps an `Outside` entry, so [`rim_resolve::domain::Resolution::scope`]
/// is never `Some(Outside)` in practice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ScopeMembershipDto {
    /// See [`ScopeMembership::Full`].
    Full,
    /// See [`ScopeMembership::Partial`].
    Partial {
        /// The owners this key names that are outside the patch's scope.
        outside: Vec<String>,
    },
}

/// Maps a resolution's [`ScopeMembership`] to the wire DTO, or `None` for
/// [`ScopeMembership::Outside`]. In practice `rim_resolve::ledger::scoped`
/// already filters every `Outside` entry out before a `Resolution` is
/// ever built (see that function's own doc comment), so a scoped
/// ledger's `Resolution::scope` is always `Some(Full | Partial)` — this
/// returns `None` rather than panicking on the case that invariant is
/// supposed to rule out, so a future change to `scoped()` degrades to a
/// missing scope badge instead of a crashed command.
#[must_use]
pub fn scope_membership_dto(value: &ScopeMembership) -> Option<ScopeMembershipDto> {
    match value {
        ScopeMembership::Full => Some(ScopeMembershipDto::Full),
        ScopeMembership::Partial { outside } => Some(ScopeMembershipDto::Partial {
            outside: outside.iter().map(|id| id.as_str().to_string()).collect(),
        }),
        ScopeMembership::Outside => None,
    }
}

/// One entry in a [`ScopeChangeDto::choices_naming_removed`] list. Mirrors
/// one element of [`ScopeChange::choices_naming_removed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ChoiceNamingRemovedDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The field whose choice named the removed owner; `null` for a
    /// `ShipAsset` decision (no per-field path).
    pub path: Option<String>,
}

/// What `update_patch`'s scope edit did to the project's existing
/// decisions. Mirrors [`ScopeChange`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ScopeChangeDto {
    /// Decisions whose key the new scope no longer admits.
    pub now_orphaned: Vec<String>,
    /// Still-admitted decisions whose choice names a removed owner.
    pub choices_naming_removed: Vec<ChoiceNamingRemovedDto>,
}

impl From<&ScopeChange> for ScopeChangeDto {
    fn from(value: &ScopeChange) -> Self {
        Self {
            now_orphaned: value.now_orphaned.iter().map(ToString::to_string).collect(),
            choices_naming_removed: value
                .choices_naming_removed
                .iter()
                .map(|(key, path)| ChoiceNamingRemovedDto {
                    key: key.to_string(),
                    path: path.as_ref().map(ToString::to_string),
                })
                .collect(),
        }
    }
}

/// One row of `list_patches`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchSummaryDto {
    /// The project's stable id.
    pub id: String,
    /// The project's label in lists.
    pub name: String,
    /// The published mod's package id.
    pub package_id: String,
    /// The published mod's display name.
    pub display_name: String,
    /// Every scope member.
    pub scope: Vec<ModRefDto>,
    /// How many findings this patch has an explicit decision on.
    pub decision_count: usize,
    /// How many of the patch's own findings are currently resolved (auto
    /// or decided).
    pub complete_count: usize,
    /// How many of the patch's own findings still need input.
    pub needs_input_count: usize,
    /// The last folder this patch was exported to, if any.
    pub export_dir: Option<String>,
    /// When this project was last changed.
    pub updated_at: String,
}

/// `get_patch`'s full detail: [`PatchSummaryDto`]'s own fields plus the
/// identity/description fields a list row doesn't need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchDetailDto {
    /// See [`PatchSummaryDto::id`].
    pub id: String,
    /// See [`PatchSummaryDto::name`].
    pub name: String,
    /// See [`PatchSummaryDto::package_id`].
    pub package_id: String,
    /// See [`PatchSummaryDto::display_name`].
    pub display_name: String,
    /// See [`PatchSummaryDto::scope`].
    pub scope: Vec<ModRefDto>,
    /// See [`PatchSummaryDto::decision_count`].
    pub decision_count: usize,
    /// See [`PatchSummaryDto::complete_count`].
    pub complete_count: usize,
    /// See [`PatchSummaryDto::needs_input_count`].
    pub needs_input_count: usize,
    /// See [`PatchSummaryDto::export_dir`].
    pub export_dir: Option<String>,
    /// See [`PatchSummaryDto::updated_at`].
    pub updated_at: String,
    /// The `About.xml` `<author>` this patch will export.
    pub author: String,
    /// The `About.xml` `<description>` this patch will export.
    pub description: String,
    /// The derived folder name this patch renders into.
    pub folder_name: String,
    /// Canonical keys of every decision this project's own scoped ledger
    /// currently considers orphaned (no longer live, or no longer
    /// admitted by the current scope) — prunable via
    /// `prune_patch_decisions`.
    pub orphaned: Vec<String>,
    /// This patch's own scoped ledger stats.
    pub stats: LedgerStatsDto,
}

/// Every scope member mapped to a [`ModRefDto`], in
/// [`PatchScope::members`]'s own id-sorted order.
fn scope_ref_dtos(scope: &PatchScope, names: &BTreeMap<ModId, String>) -> Vec<ModRefDto> {
    scope
        .members()
        .iter()
        .map(|id| ModRefDto {
            mod_id: id.as_str().to_string(),
            name: names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.as_str().to_string()),
        })
        .collect()
}

/// Builds a [`PatchSummaryDto`] from a loaded [`PatchProject`], its own
/// scoped ledger stats, and a mod-id -> display-name map.
#[must_use]
pub fn patch_summary_dto(
    project: &PatchProject,
    stats: LedgerStats,
    names: &BTreeMap<ModId, String>,
) -> PatchSummaryDto {
    PatchSummaryDto {
        id: project.id().to_string(),
        name: project.name().to_string(),
        package_id: project.identity().package_id().as_str().to_string(),
        display_name: project.identity().display_name().to_string(),
        scope: scope_ref_dtos(project.scope(), names),
        decision_count: project.decisions().len(),
        complete_count: stats.auto + stats.overridden,
        needs_input_count: stats.needs_input,
        export_dir: project.export_dir().map(|dir| dir.display().to_string()),
        updated_at: project.updated_at().to_string(),
    }
}

/// Builds a [`PatchDetailDto`] from a loaded [`PatchProject`], its own
/// scoped ledger stats, a mod-id -> display-name map, and the decisions
/// its own project currently considers orphaned
/// ([`PatchProject::orphaned`], already filtered by the caller against the
/// live scoped-ledger keys).
#[must_use]
pub fn patch_detail_dto(
    project: &PatchProject,
    stats: LedgerStats,
    names: &BTreeMap<ModId, String>,
    orphaned: &[FindingKey],
) -> PatchDetailDto {
    let summary = patch_summary_dto(project, stats, names);
    PatchDetailDto {
        id: summary.id,
        name: summary.name,
        package_id: summary.package_id,
        display_name: summary.display_name,
        scope: summary.scope,
        decision_count: summary.decision_count,
        complete_count: summary.complete_count,
        needs_input_count: summary.needs_input_count,
        export_dir: summary.export_dir,
        updated_at: summary.updated_at,
        author: project.author().to_string(),
        description: project.description().to_string(),
        folder_name: project.identity().folder_name().to_string(),
        orphaned: orphaned.iter().map(ToString::to_string).collect(),
        stats: stats.into(),
    }
}

/// Request shape for `create_patch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CreatePatchRequestDto {
    /// The project's label in lists.
    pub name: String,
    /// The published mod's package id.
    pub package_id: String,
    /// The published mod's display name.
    pub display_name: String,
    /// The patch scope's members, by mod id.
    pub scope: Vec<String>,
}

impl From<CreatePatchRequestDto> for CreatePatchInput {
    fn from(value: CreatePatchRequestDto) -> Self {
        Self {
            name: value.name,
            package_id: value.package_id,
            display_name: value.display_name,
            scope: value.scope.into_iter().map(ModId::new).collect(),
        }
    }
}

/// Request shape for `update_patch`. `None` on any field means "leave
/// unchanged".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct UpdatePatchRequestDto {
    /// Which patch to update.
    pub patch_id: String,
    /// A new list label.
    pub name: Option<String>,
    /// A new published package id.
    pub package_id: Option<String>,
    /// A new published display name.
    pub display_name: Option<String>,
    /// A new `About.xml` author.
    pub author: Option<String>,
    /// A new `About.xml` description.
    pub description: Option<String>,
    /// A new scope, by mod id.
    pub scope: Option<Vec<String>>,
}

impl From<UpdatePatchRequestDto> for UpdatePatchInput {
    fn from(value: UpdatePatchRequestDto) -> Self {
        Self {
            name: value.name,
            package_id: value.package_id,
            display_name: value.display_name,
            author: value.author,
            description: value.description,
            scope: value
                .scope
                .map(|ids| ids.into_iter().map(ModId::new).collect()),
        }
    }
}

/// What `update_patch` returns: the patch's fresh detail, plus the scope
/// change it made when `scope` was part of the update (`null` otherwise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchUpdateResultDto {
    /// The patch's fresh detail.
    pub patch: PatchDetailDto,
    /// The scope change this update made, if `scope` was part of it.
    pub scope_change: Option<ScopeChangeDto>,
}

/// Request shape for `decide_patch`. Mirrors
/// [`super::finding::DecideRequestDto`] plus which patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DecidePatchRequestDto {
    /// Which patch to decide on.
    pub patch_id: String,
    /// The finding's canonical key text.
    pub key: String,
    /// The chosen action — one of `Merge`/`ShipAsset`/`Ignore` (any other
    /// [`ActionDto`] is rejected by
    /// [`rim_resolve::domain::PatchProject::decide`]).
    pub action: ActionDto,
    /// An optional free-text note.
    pub note: Option<String>,
}

/// Request shape for `list_patch_findings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchFindingsRequestDto {
    /// Which patch's own scoped ledger to page.
    pub patch_id: String,
    /// Filters and pages the ledger.
    pub filter: FindingFilterDto,
}

/// Request shape for `import_profile_decisions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportProfileDecisionsRequestDto {
    /// Which patch to import into.
    pub patch_id: String,
    /// The keys to import, or `null` for every key the patch's own scoped
    /// ledger admits.
    pub keys: Option<Vec<String>>,
}

/// One key [`ImportProfileDecisions`](rim_session::use_cases::ImportProfileDecisions)
/// skipped, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SkippedImportDto {
    /// The finding's canonical key text.
    pub key: String,
    /// Why the profile's decision on this key was rejected.
    pub cause: DecisionRejectionDto,
}

/// Why a patch's scope rejected a decision. Mirrors [`PatchDecisionError`];
/// the frontend renders the sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DecisionRejectionDto {
    /// See [`PatchDecisionError::OutOfScope`].
    OutOfScope,
    /// See [`PatchDecisionError::NotPatchable`].
    NotPatchable {
        /// The rejected action kind.
        action: UnpatchableActionDto,
    },
    /// See [`PatchDecisionError::ChoiceOutsideScope`].
    ChoiceOutsideScope {
        /// The field whose choice named the out-of-scope owner. Struct
        /// variant fields need their own `rename` for camelCase (see
        /// `CommandErrorDetail`).
        path: String,
        /// The out-of-scope owner the choice named.
        #[serde(rename = "modId")]
        mod_id: String,
    },
    /// See [`PatchDecisionError::AssetOutsideScope`].
    AssetOutsideScope {
        /// The out-of-scope owner the asset would be copied from.
        #[serde(rename = "modId")]
        mod_id: String,
    },
}

/// An action kind a compat patch can never carry. Mirrors
/// [`UnpatchableAction`]; the frontend renders the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UnpatchableActionDto {
    /// `Action::Accept`.
    Accept,
    /// `Action::Reorder`.
    Reorder,
    /// `Action::PreferWinner`.
    PreferWinner,
    /// `Action::ChooseCandidate`.
    ChooseCandidate,
    /// `Action::DropEdge`.
    DropEdge,
    /// `Action::KeepEdge`.
    KeepEdge,
    /// `Action::AddTag`.
    AddTag,
    /// `Action::RemoveTag`.
    RemoveTag,
    /// `Action::ExcludeFromCluster`.
    ExcludeFromCluster,
    /// `Action::RemoveMod`.
    RemoveMod,
    /// `Action::PromoteRule`.
    PromoteRule,
    /// `Action::DropRule`.
    DropRule,
}

impl From<UnpatchableAction> for UnpatchableActionDto {
    fn from(value: UnpatchableAction) -> Self {
        match value {
            UnpatchableAction::Accept => Self::Accept,
            UnpatchableAction::Reorder => Self::Reorder,
            UnpatchableAction::PreferWinner => Self::PreferWinner,
            UnpatchableAction::ChooseCandidate => Self::ChooseCandidate,
            UnpatchableAction::DropEdge => Self::DropEdge,
            UnpatchableAction::KeepEdge => Self::KeepEdge,
            UnpatchableAction::AddTag => Self::AddTag,
            UnpatchableAction::RemoveTag => Self::RemoveTag,
            UnpatchableAction::ExcludeFromCluster => Self::ExcludeFromCluster,
            UnpatchableAction::RemoveMod => Self::RemoveMod,
            UnpatchableAction::PromoteRule => Self::PromoteRule,
            UnpatchableAction::DropRule => Self::DropRule,
        }
    }
}

impl From<&PatchDecisionError> for DecisionRejectionDto {
    fn from(value: &PatchDecisionError) -> Self {
        match value {
            PatchDecisionError::OutOfScope(_) => Self::OutOfScope,
            PatchDecisionError::NotPatchable(action) => Self::NotPatchable {
                action: (*action).into(),
            },
            PatchDecisionError::ChoiceOutsideScope { path, mod_id } => Self::ChoiceOutsideScope {
                path: path.to_string(),
                mod_id: mod_id.to_string(),
            },
            PatchDecisionError::AssetOutsideScope(mod_id) => Self::AssetOutsideScope {
                mod_id: mod_id.to_string(),
            },
        }
    }
}

/// What `import_profile_decisions` did. Mirrors [`ImportReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportDecisionsReportDto {
    /// Keys whose profile decision was copied into the patch.
    pub imported: Vec<String>,
    /// Keys whose profile decision the patch's scope rejected, and why.
    pub skipped: Vec<SkippedImportDto>,
}

impl From<&ImportReport> for ImportDecisionsReportDto {
    fn from(value: &ImportReport) -> Self {
        Self {
            imported: value.imported.iter().map(ToString::to_string).collect(),
            skipped: value
                .skipped
                .iter()
                .map(|(key, rejection)| SkippedImportDto {
                    key: key.to_string(),
                    cause: rejection.into(),
                })
                .collect(),
        }
    }
}

/// `get_patch_render`'s result: what `export_patch` would currently
/// produce for this patch, described for the export panel/file preview.
/// Mirrors [`MergeModRender`] the way [`super::merge::MergeModDto`]
/// mirrors it for the profile merge mod, plus `caveats` (the union of
/// every entry's own preview caveats, which [`MergeModRender`] doesn't
/// aggregate) and no `exists`/`mods_path` (a patch's export target is
/// user-chosen, not a fixed `Mods/<folder>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchRenderDto {
    /// The published mod's package id.
    pub package_id: String,
    /// The derived folder name this patch renders into.
    pub folder_name: String,
    /// The published mod's display name.
    pub display_name: String,
    /// Exactly the patch's own scope (`Dependencies::Exactly` — see
    /// [`MergeModRender::dependencies`]).
    pub dependencies: Vec<MergeSourceModDto>,
    /// One entry per `Merge`/`ShipAsset` decision.
    pub entries: Vec<MergeModEntryDto>,
    /// Relative paths a render would produce — empty when nothing
    /// renders.
    pub files: Vec<String>,
    /// `Merge`/`ShipAsset` decisions skipped because their preview wasn't
    /// complete, or their asset couldn't be located.
    pub skipped: Vec<String>,
    /// The canonical hash of the decisions this render was built from.
    pub decisions_sha256: String,
    /// Every entry's own plan caveats, structured.
    pub caveats: Vec<CaveatDto>,
}

/// Builds a [`PatchRenderDto`] from [`RenderPatch`](rim_session::use_cases::RenderPatch)'s
/// output, the project it rendered, a mod-id -> display-name map, and the
/// already-collected caveat text (the command's own job — it alone can
/// reach each entry's cached preview via [`rim_session::Session::merge_preview`]).
#[must_use]
pub fn patch_render_dto(
    render: &MergeModRender,
    project: &PatchProject,
    names: &BTreeMap<ModId, String>,
    caveats: Vec<CaveatDto>,
) -> PatchRenderDto {
    let dependencies = render
        .dependencies
        .iter()
        .map(|id| MergeSourceModDto {
            mod_id: id.as_str().to_string(),
            name: names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.as_str().to_string()),
        })
        .collect();
    let entries = render.entries.iter().map(MergeModEntryDto::from).collect();
    let files = render
        .rendered
        .as_ref()
        .map(|rendered| {
            rendered
                .files
                .iter()
                .map(|file| file.relative_path.display().to_string())
                .collect()
        })
        .unwrap_or_default();

    PatchRenderDto {
        package_id: project.identity().package_id().as_str().to_string(),
        folder_name: project.identity().folder_name().to_string(),
        display_name: project.identity().display_name().to_string(),
        dependencies,
        entries,
        files,
        skipped: render.skipped.iter().map(ToString::to_string).collect(),
        decisions_sha256: render.decisions_sha256.clone(),
        caveats,
    }
}

/// Request shape for `export_patch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportPatchRequestDto {
    /// Which patch to export.
    pub patch_id: String,
    /// The folder to render the patch into — never the game's `Mods`
    /// folder or a subfolder of it.
    pub out_dir: String,
    /// Also copy the same rendered folder into the game's `Mods` folder
    /// and activate it.
    pub install: bool,
    /// Skip the running-game refusal on `install` (ignored when `install`
    /// is `false`).
    pub force: bool,
}

/// What `export_patch` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportPatchReportDto {
    /// Where the patch was written, for display.
    pub export_path: String,
    /// Where the patch was installed, when `install` was set.
    pub installed_path: Option<String>,
    /// The `ModsConfig.xml` backup path, when `install` wrote it.
    pub mods_config_backup_path: Option<String>,
    /// `Merge`/`ShipAsset` decisions skipped because their preview wasn't
    /// complete, or their asset couldn't be located.
    pub skipped: Vec<String>,
    /// Every written file's path, relative to `export_path`.
    pub files: Vec<String>,
    /// Canonical hash of the decisions this export was rendered from
    /// ([`rim_resolve::domain::PatchProject::decisions_sha256`]) — lets the
    /// UI detect "unchanged since last export" across a reload by comparing
    /// this value to the current render's own hash, rather than only
    /// within the lifetime of one open panel.
    pub decisions_sha256: String,
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{PatchId, PatchModIdentity, PatchScope};

    use super::*;

    fn test_project() -> PatchProject {
        let identity =
            PatchModIdentity::new("sample.abcompat", "A + B Compat").expect("valid identity");
        let scope = PatchScope::new([ModId::new("fixture.moda"), ModId::new("fixture.modb")])
            .expect("two distinct members");
        let id: PatchId = "3f9a1c02be77".parse().expect("valid patch id");
        PatchProject::new(
            id,
            "AB compat".to_string(),
            identity,
            scope,
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn scope_membership_dto_maps_full() {
        let dto = scope_membership_dto(&ScopeMembership::Full);
        assert_eq!(dto, Some(ScopeMembershipDto::Full));
    }

    #[test]
    fn scope_membership_dto_maps_partial_with_outside_owners() {
        let membership = ScopeMembership::Partial {
            outside: [ModId::new("c.mod")].into_iter().collect(),
        };
        let dto = scope_membership_dto(&membership);
        assert_eq!(
            dto,
            Some(ScopeMembershipDto::Partial {
                outside: vec!["c.mod".to_string()]
            })
        );
    }

    #[test]
    fn scope_membership_dto_is_none_for_outside() {
        let dto = scope_membership_dto(&ScopeMembership::Outside);
        assert_eq!(dto, None);
    }

    #[test]
    fn patch_summary_dto_carries_scope_names_and_counts() {
        let project = test_project();
        let names = BTreeMap::from([
            (ModId::new("fixture.moda"), "Mod A".to_string()),
            (ModId::new("fixture.modb"), "Mod B".to_string()),
        ]);
        let stats = LedgerStats {
            auto: 1,
            needs_input: 2,
            overridden: 3,
            resolved_by_suggested: 0,
            merged: 0,
            merge_incomplete: 0,
        };

        let dto = patch_summary_dto(&project, stats, &names);

        assert_eq!(dto.id, "3f9a1c02be77");
        assert_eq!(dto.package_id, "sample.abcompat");
        assert_eq!(
            dto.scope,
            vec![
                ModRefDto {
                    mod_id: "fixture.moda".to_string(),
                    name: "Mod A".to_string(),
                },
                ModRefDto {
                    mod_id: "fixture.modb".to_string(),
                    name: "Mod B".to_string(),
                },
            ]
        );
        assert_eq!(dto.decision_count, 0);
        assert_eq!(dto.complete_count, 4, "auto + overridden");
        assert_eq!(dto.needs_input_count, 2);
        assert_eq!(dto.export_dir, None);
    }

    #[test]
    fn patch_detail_dto_carries_the_orphaned_keys_and_stats_through() {
        let project = test_project();
        let names = BTreeMap::new();
        let stats = LedgerStats::default();
        let orphaned = vec![FindingKey::MissingMod {
            mod_id: ModId::new("gone.mod"),
        }];

        let dto = patch_detail_dto(&project, stats, &names, &orphaned);

        assert_eq!(dto.author, "Rimmerge");
        assert_eq!(dto.folder_name, "sample_abcompat");
        assert_eq!(dto.orphaned, vec!["missing_mod:gone.mod".to_string()]);
        assert_eq!(dto.stats, LedgerStatsDto::default());
    }

    #[test]
    fn create_patch_request_dto_maps_into_the_use_case_input() {
        let request = CreatePatchRequestDto {
            name: "AB compat".to_string(),
            package_id: "sample.abcompat".to_string(),
            display_name: "A + B Compat".to_string(),
            scope: vec!["fixture.moda".to_string(), "fixture.modb".to_string()],
        };

        let input: CreatePatchInput = request.into();

        assert_eq!(input.name, "AB compat");
        assert_eq!(
            input.scope,
            [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn update_patch_request_dto_leaves_unset_fields_none() {
        let request = UpdatePatchRequestDto {
            patch_id: "3f9a1c02be77".to_string(),
            name: Some("renamed".to_string()),
            package_id: None,
            display_name: None,
            author: None,
            description: None,
            scope: None,
        };

        let input: UpdatePatchInput = request.into();

        assert_eq!(input.name, Some("renamed".to_string()));
        assert_eq!(input.package_id, None);
        assert_eq!(input.scope, None);
    }

    #[test]
    fn import_decisions_report_dto_maps_imported_and_skipped() {
        let report = ImportReport {
            imported: vec![FindingKey::MissingMod {
                mod_id: ModId::new("a.mod"),
            }],
            skipped: vec![(
                FindingKey::MissingMod {
                    mod_id: ModId::new("b.mod"),
                },
                rim_resolve::domain::PatchDecisionError::NotPatchable(
                    rim_resolve::domain::UnpatchableAction::Accept,
                ),
            )],
        };

        let dto: ImportDecisionsReportDto = (&report).into();

        assert_eq!(dto.imported, vec!["missing_mod:a.mod".to_string()]);
        assert_eq!(dto.skipped.len(), 1);
        assert_eq!(dto.skipped[0].key, "missing_mod:b.mod");
        assert_eq!(
            dto.skipped[0].cause,
            DecisionRejectionDto::NotPatchable {
                action: UnpatchableActionDto::Accept
            }
        );
    }

    #[test]
    fn a_choice_outside_scope_rejection_keeps_the_field_and_the_mod() {
        let rejection = PatchDecisionError::ChoiceOutsideScope {
            path: "label".parse().expect("valid field path"),
            mod_id: ModId::new("c.mod"),
        };

        let dto = DecisionRejectionDto::from(&rejection);

        assert_eq!(
            dto,
            DecisionRejectionDto::ChoiceOutsideScope {
                path: "label".to_string(),
                mod_id: "c.mod".to_string()
            }
        );
        let wire = serde_json::to_value(&dto).expect("serializes");
        assert_eq!(wire["kind"], "choiceOutsideScope");
        assert_eq!(wire["modId"], "c.mod");
    }

    #[test]
    fn scope_change_dto_maps_orphaned_keys_and_removed_choices() {
        let change = ScopeChange {
            now_orphaned: vec![FindingKey::MissingMod {
                mod_id: ModId::new("a.mod"),
            }],
            choices_naming_removed: vec![(
                FindingKey::MissingMod {
                    mod_id: ModId::new("b.mod"),
                },
                Some("label".parse().expect("valid field path")),
            )],
        };

        let dto: ScopeChangeDto = (&change).into();

        assert_eq!(dto.now_orphaned, vec!["missing_mod:a.mod".to_string()]);
        assert_eq!(dto.choices_naming_removed.len(), 1);
        assert_eq!(dto.choices_naming_removed[0].key, "missing_mod:b.mod");
        assert_eq!(dto.choices_naming_removed[0].path.as_deref(), Some("label"));
    }
}
