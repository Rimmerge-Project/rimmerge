//! Finding kinds, filters, pages, per-kind counts, and ledger stats: the DTOs that address findings.

use rim_resolve::domain::Resolution;
use rim_session::{FindingFilter, FindingKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::suggestion::ResolutionSummaryDto;
use crate::dto::common::ResolutionStatusDto;

/// Mirrors [`FindingKind`], for [`FindingFilterDto::kinds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FindingKindDto {
    /// See [`FindingKind::EdgeDropped`].
    EdgeDropped,
    /// See [`FindingKind::AnyOfChoice`].
    AnyOfChoice,
    /// See [`FindingKind::DefOverride`].
    DefOverride,
    /// See [`FindingKind::PatchCollision`].
    PatchCollision,
    /// See [`FindingKind::TextureOverride`].
    TextureOverride,
    /// See [`FindingKind::DuplicateAssembly`].
    DuplicateAssembly,
    /// See [`FindingKind::LikelyDuplicateMod`].
    LikelyDuplicateMod,
    /// See [`FindingKind::MissingMod`].
    MissingMod,
    /// See [`FindingKind::MissingDependency`].
    MissingDependency,
    /// See [`FindingKind::IncompatiblePair`].
    IncompatiblePair,
    /// See [`FindingKind::UnsupportedVersion`].
    UnsupportedVersion,
    /// See [`FindingKind::UndeclaredHardDependency`].
    UndeclaredHardDependency,
    /// See [`FindingKind::LazyReferenceViolated`].
    LazyReferenceViolated,
    /// See [`FindingKind::DeclarationQuestioned`].
    DeclarationQuestioned,
    /// See [`FindingKind::DeclarationOverridden`].
    DeclarationOverridden,
    /// See [`FindingKind::DuplicateTemplateName`].
    DuplicateTemplateName,
    /// See [`FindingKind::KeyedTranslationCollision`].
    KeyedTranslationCollision,
    /// See [`FindingKind::SoundOverride`].
    SoundOverride,
    /// See [`FindingKind::UndeclaredTypeDependency`].
    UndeclaredTypeDependency,
    /// See [`FindingKind::RuntimePatchCollision`].
    RuntimePatchCollision,
    /// See [`FindingKind::TranspilerCollision`].
    TranspilerCollision,
    /// See [`FindingKind::TagInferred`].
    TagInferred,
    /// See [`FindingKind::RuleOverruled`].
    RuleOverruled,
    /// See [`FindingKind::PlacementOverruled`].
    PlacementOverruled,
    /// See [`FindingKind::PlacementQuestioned`].
    PlacementQuestioned,
    /// See [`FindingKind::PlacementOrderingOverridden`].
    PlacementOrderingOverridden,
    /// See [`FindingKind::PlacementPromotesDependents`].
    PlacementPromotesDependents,
    /// See [`FindingKind::MissingTexturePath`].
    MissingTexturePath,
    /// See [`FindingKind::PatchWillFail`].
    PatchWillFail,
    /// See [`FindingKind::ContributesNothing`].
    ContributesNothing,
    /// See [`FindingKind::UndecodableTexture`].
    UndecodableTexture,
    /// See [`FindingKind::BrokenInheritance`].
    BrokenInheritance,
    /// See [`FindingKind::NearMissModReference`].
    NearMissModReference,
    /// See [`FindingKind::DiscardedAddition`].
    DiscardedAddition,
    /// See [`FindingKind::DanglingDefReference`].
    DanglingDefReference,
}

impl From<FindingKindDto> for FindingKind {
    fn from(value: FindingKindDto) -> Self {
        match value {
            FindingKindDto::EdgeDropped => Self::EdgeDropped,
            FindingKindDto::AnyOfChoice => Self::AnyOfChoice,
            FindingKindDto::DefOverride => Self::DefOverride,
            FindingKindDto::PatchCollision => Self::PatchCollision,
            FindingKindDto::TextureOverride => Self::TextureOverride,
            FindingKindDto::DuplicateAssembly => Self::DuplicateAssembly,
            FindingKindDto::LikelyDuplicateMod => Self::LikelyDuplicateMod,
            FindingKindDto::MissingMod => Self::MissingMod,
            FindingKindDto::MissingDependency => Self::MissingDependency,
            FindingKindDto::IncompatiblePair => Self::IncompatiblePair,
            FindingKindDto::UnsupportedVersion => Self::UnsupportedVersion,
            FindingKindDto::UndeclaredHardDependency => Self::UndeclaredHardDependency,
            FindingKindDto::LazyReferenceViolated => Self::LazyReferenceViolated,
            FindingKindDto::DeclarationQuestioned => Self::DeclarationQuestioned,
            FindingKindDto::DeclarationOverridden => Self::DeclarationOverridden,
            FindingKindDto::DuplicateTemplateName => Self::DuplicateTemplateName,
            FindingKindDto::KeyedTranslationCollision => Self::KeyedTranslationCollision,
            FindingKindDto::SoundOverride => Self::SoundOverride,
            FindingKindDto::UndeclaredTypeDependency => Self::UndeclaredTypeDependency,
            FindingKindDto::RuntimePatchCollision => Self::RuntimePatchCollision,
            FindingKindDto::TranspilerCollision => Self::TranspilerCollision,
            FindingKindDto::TagInferred => Self::TagInferred,
            FindingKindDto::RuleOverruled => Self::RuleOverruled,
            FindingKindDto::PlacementOverruled => Self::PlacementOverruled,
            FindingKindDto::PlacementQuestioned => Self::PlacementQuestioned,
            FindingKindDto::PlacementOrderingOverridden => Self::PlacementOrderingOverridden,
            FindingKindDto::PlacementPromotesDependents => Self::PlacementPromotesDependents,
            FindingKindDto::MissingTexturePath => Self::MissingTexturePath,
            FindingKindDto::PatchWillFail => Self::PatchWillFail,
            FindingKindDto::ContributesNothing => Self::ContributesNothing,
            FindingKindDto::UndecodableTexture => Self::UndecodableTexture,
            FindingKindDto::BrokenInheritance => Self::BrokenInheritance,
            FindingKindDto::NearMissModReference => Self::NearMissModReference,
            FindingKindDto::DiscardedAddition => Self::DiscardedAddition,
            FindingKindDto::DanglingDefReference => Self::DanglingDefReference,
        }
    }
}

/// Request shape for `list_findings`. Mirrors [`FindingFilter`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FindingFilterDto {
    /// Only findings with this status.
    pub status: Option<ResolutionStatusDto>,
    /// Only findings of one of these kinds.
    pub kinds: Option<Vec<FindingKindDto>>,
    /// Only findings naming this mod.
    pub mod_id: Option<String>,
    /// Only findings whose canonical key text contains this substring.
    pub search: Option<String>,
    /// How many matching entries to skip.
    pub offset: usize,
    /// How many entries to return, capped at
    /// [`rim_session::MAX_PAGE_SIZE`].
    pub limit: usize,
}

impl From<FindingFilterDto> for FindingFilter {
    fn from(value: FindingFilterDto) -> Self {
        use rim_analyzer::domain::ModId;

        Self {
            status: value.status.map(|status| match status {
                ResolutionStatusDto::Auto => rim_resolve::domain::ResolutionStatus::Auto,
                ResolutionStatusDto::NeedsInput => {
                    rim_resolve::domain::ResolutionStatus::NeedsInput
                }
                ResolutionStatusDto::UserOverridden => {
                    rim_resolve::domain::ResolutionStatus::UserOverridden
                }
            }),
            kinds: value
                .kinds
                .map(|kinds| kinds.into_iter().map(Into::into).collect()),
            mod_id: value.mod_id.map(ModId::new),
            search: value.search,
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// One page of `list_findings`. Mirrors
/// [`rim_session::FindingPage`], with each key already resolved to its
/// full [`ResolutionSummaryDto`] (built by the command from
/// [`rim_session::Session::resolution`] — a page's
/// [`rim_resolve::domain::FindingKey`]s alone
/// aren't enough to render a list row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FindingPageDto {
    /// Total entries matching the filter, before paging.
    pub total: usize,
    /// This page's summaries, in the ledger's stable sort order.
    pub items: Vec<ResolutionSummaryDto>,
}

/// Ledger stats for one source. Mirrors
/// [`rim_resolve::domain::LedgerStats`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LedgerStatsDto {
    /// Entries whose suggestion met the confidence threshold.
    pub auto: usize,
    /// Entries still awaiting a decision.
    pub needs_input: usize,
    /// Entries the user has decided.
    pub overridden: usize,
    /// Entries the suggested order would already resolve.
    pub resolved_by_suggested: usize,
}

impl From<rim_resolve::domain::LedgerStats> for LedgerStatsDto {
    fn from(value: rim_resolve::domain::LedgerStats) -> Self {
        Self {
            auto: value.auto,
            needs_input: value.needs_input,
            overridden: value.overridden,
            resolved_by_suggested: value.resolved_by_suggested,
        }
    }
}

/// Needs-input finding counts bucketed by [`FindingKind`], for the
/// dashboard's "top needs-input kinds" summary. A fixed-field struct like
/// [`super::dashboard::ConflictKindCountsDto`](crate::dto::dashboard::ConflictKindCountsDto), for the same reason: the
/// bucket set is small, closed, and stable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FindingKindCountsDto {
    /// See [`FindingKind::EdgeDropped`].
    pub edge_dropped: usize,
    /// See [`FindingKind::AnyOfChoice`].
    pub any_of_choice: usize,
    /// See [`FindingKind::DefOverride`].
    pub def_override: usize,
    /// See [`FindingKind::PatchCollision`].
    pub patch_collision: usize,
    /// See [`FindingKind::TextureOverride`].
    pub texture_override: usize,
    /// See [`FindingKind::DuplicateAssembly`].
    pub duplicate_assembly: usize,
    /// See [`FindingKind::LikelyDuplicateMod`].
    pub likely_duplicate_mod: usize,
    /// See [`FindingKind::MissingMod`].
    pub missing_mod: usize,
    /// See [`FindingKind::MissingDependency`].
    pub missing_dependency: usize,
    /// See [`FindingKind::IncompatiblePair`].
    pub incompatible_pair: usize,
    /// See [`FindingKind::UnsupportedVersion`].
    pub unsupported_version: usize,
    /// See [`FindingKind::UndeclaredHardDependency`].
    pub undeclared_hard_dependency: usize,
    /// See [`FindingKind::LazyReferenceViolated`].
    pub lazy_reference_violated: usize,
    /// See [`FindingKind::DeclarationQuestioned`].
    pub declaration_questioned: usize,
    /// See [`FindingKind::DeclarationOverridden`].
    pub declaration_overridden: usize,
    /// See [`FindingKind::DuplicateTemplateName`].
    pub duplicate_template_name: usize,
    /// See [`FindingKind::KeyedTranslationCollision`].
    pub keyed_translation_collision: usize,
    /// See [`FindingKind::SoundOverride`].
    pub sound_override: usize,
    /// See [`FindingKind::UndeclaredTypeDependency`].
    pub undeclared_type_dependency: usize,
    /// See [`FindingKind::RuntimePatchCollision`].
    pub runtime_patch_collision: usize,
    /// See [`FindingKind::TranspilerCollision`].
    pub transpiler_collision: usize,
    /// See [`FindingKind::TagInferred`].
    pub tag_inferred: usize,
    /// See [`FindingKind::RuleOverruled`].
    pub rule_overruled: usize,
    /// See [`FindingKind::PlacementOverruled`].
    pub placement_overruled: usize,
    /// See [`FindingKind::PlacementQuestioned`].
    pub placement_questioned: usize,
    /// See [`FindingKind::PlacementOrderingOverridden`].
    pub placement_ordering_overridden: usize,
    /// See [`FindingKind::PlacementPromotesDependents`].
    pub placement_promotes_dependents: usize,
    /// See [`FindingKind::MissingTexturePath`].
    pub missing_texture_path: usize,
    /// See [`FindingKind::PatchWillFail`].
    pub patch_will_fail: usize,
    /// See [`FindingKind::ContributesNothing`].
    pub contributes_nothing: usize,
    /// See [`FindingKind::UndecodableTexture`].
    pub undecodable_texture: usize,
    /// See [`FindingKind::BrokenInheritance`].
    pub broken_inheritance: usize,
    /// See [`FindingKind::NearMissModReference`].
    pub near_miss_mod_reference: usize,
    /// See [`FindingKind::DiscardedAddition`].
    pub discarded_addition: usize,
    /// See [`FindingKind::DanglingDefReference`].
    pub dangling_def_reference: usize,
}

impl FindingKindCountsDto {
    fn add(&mut self, kind: FindingKind) {
        match kind {
            FindingKind::EdgeDropped => self.edge_dropped += 1,
            FindingKind::AnyOfChoice => self.any_of_choice += 1,
            FindingKind::DefOverride => self.def_override += 1,
            FindingKind::PatchCollision => self.patch_collision += 1,
            FindingKind::TextureOverride => self.texture_override += 1,
            FindingKind::DuplicateAssembly => self.duplicate_assembly += 1,
            FindingKind::LikelyDuplicateMod => self.likely_duplicate_mod += 1,
            FindingKind::MissingMod => self.missing_mod += 1,
            FindingKind::MissingDependency => self.missing_dependency += 1,
            FindingKind::IncompatiblePair => self.incompatible_pair += 1,
            FindingKind::UnsupportedVersion => self.unsupported_version += 1,
            FindingKind::UndeclaredHardDependency => self.undeclared_hard_dependency += 1,
            FindingKind::LazyReferenceViolated => self.lazy_reference_violated += 1,
            FindingKind::DeclarationQuestioned => self.declaration_questioned += 1,
            FindingKind::DeclarationOverridden => self.declaration_overridden += 1,
            FindingKind::DuplicateTemplateName => self.duplicate_template_name += 1,
            FindingKind::KeyedTranslationCollision => self.keyed_translation_collision += 1,
            FindingKind::SoundOverride => self.sound_override += 1,
            FindingKind::UndeclaredTypeDependency => self.undeclared_type_dependency += 1,
            FindingKind::RuntimePatchCollision => self.runtime_patch_collision += 1,
            FindingKind::TranspilerCollision => self.transpiler_collision += 1,
            FindingKind::TagInferred => self.tag_inferred += 1,
            FindingKind::RuleOverruled => self.rule_overruled += 1,
            FindingKind::PlacementOverruled => self.placement_overruled += 1,
            FindingKind::PlacementQuestioned => self.placement_questioned += 1,
            FindingKind::PlacementOrderingOverridden => self.placement_ordering_overridden += 1,
            FindingKind::PlacementPromotesDependents => self.placement_promotes_dependents += 1,
            FindingKind::MissingTexturePath => self.missing_texture_path += 1,
            FindingKind::PatchWillFail => self.patch_will_fail += 1,
            FindingKind::ContributesNothing => self.contributes_nothing += 1,
            FindingKind::UndecodableTexture => self.undecodable_texture += 1,
            FindingKind::BrokenInheritance => self.broken_inheritance += 1,
            FindingKind::NearMissModReference => self.near_miss_mod_reference += 1,
            FindingKind::DiscardedAddition => self.discarded_addition += 1,
            FindingKind::DanglingDefReference => self.dangling_def_reference += 1,
        }
    }
}

/// Buckets every `NeedsInput` entry in `entries` (one order's ledger) by
/// [`FindingKind`].
#[must_use]
pub fn needs_input_by_kind(entries: &[Resolution]) -> FindingKindCountsDto {
    let mut counts = FindingKindCountsDto::default();
    for entry in entries {
        if entry.status == rim_resolve::domain::ResolutionStatus::NeedsInput {
            counts.add(FindingKind::of(&entry.key));
        }
    }
    counts
}
