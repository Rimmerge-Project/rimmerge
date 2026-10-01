//! [`RationaleDto`]: the structured "why" behind a [`super::AlternativeDto`]/
//! [`super::SuggestionDto`]/[`crate::dto::verify::VerifyReorderDto`].
//! Mirrors [`rim_resolve::domain::Rationale`] one variant at a time — the
//! frontend renders a localized sentence from a variant's own typed
//! fields, never from English prose crossing IPC. A DTO variant appears
//! here exactly when its `Rationale` counterpart does, so this enum
//! stays exhaustive over `Rationale` with no speculative future
//! variants.
//!
//! Every DTO here still carries its existing plain-English `rationale:
//! String` field too, kept forever for the pair-rule `--comment` prefill
//! (`useApplyDialog.ts`) — this `rationaleCode` field is additive, for
//! rendering only.

use rim_resolve::domain::Rationale;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::evidence::{DanglingCauseDto, EdgeWinnerDto, ModReferenceKindDto};
use crate::dto::common::{EdgeKindDto, PlacementDto, RuleOriginDto};
use crate::dto::order::LayerDto;

/// See this module's own doc comment. Mirrors [`Rationale`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RationaleDto {
    // -- edges.rs --
    /// See [`Rationale::EdgeDroppedInferred`].
    #[serde(rename_all = "camelCase")]
    EdgeDroppedInferred {
        /// Which analyzer rule inferred the edge.
        edge_kind: EdgeKindDto,
        /// The edge's dependent side.
        after: String,
        /// The edge's dependency side.
        before: String,
    },
    /// See [`Rationale::EdgeDroppedCosmetic`].
    EdgeDroppedCosmetic,
    /// See [`Rationale::KeepEdgeInCycle`].
    KeepEdgeInCycle,
    /// See [`Rationale::ReorderOverridingEdgeWinner`].
    ReorderOverridingEdgeWinner,
    /// See [`Rationale::EdgeDroppedWithWinner`].
    #[serde(rename_all = "camelCase")]
    EdgeDroppedWithWinner {
        /// The dropped edge's dependent side.
        after: String,
        /// The dropped edge's dependency side.
        before: String,
        /// The edge that overruled it.
        winner: EdgeWinnerDto,
    },
    /// See [`Rationale::EdgeDroppedSoftOrAwareness`].
    EdgeDroppedSoftOrAwareness,
    /// See [`Rationale::EdgeDroppedDeclared`].
    EdgeDroppedDeclared,
    /// See [`Rationale::EdgeDroppedHard`].
    EdgeDroppedHard,
    /// See [`Rationale::RemoveDependentModBrokenOrder`].
    RemoveDependentModBrokenOrder,
    /// See [`Rationale::DeclarationQuestioned`].
    #[serde(rename_all = "camelCase")]
    DeclarationQuestioned {
        /// The declared edge's dependent side.
        declared_after: String,
        /// The declared edge's dependency side.
        declared_before: String,
        /// The declared edge's own layer.
        declared_layer: LayerDto,
        /// The declared edge's own detail text.
        declared_detail: String,
        /// The advisory relation's own kind.
        relation_kind: EdgeKindDto,
        /// The advisory relation's own detail text.
        relation_detail: String,
    },
    /// See [`Rationale::ReorderOppositeOfRelation`].
    ReorderOppositeOfRelation,
    /// See [`Rationale::DeclarationOverridden`].
    #[serde(rename_all = "camelCase")]
    DeclarationOverridden {
        /// The declared edge's dependent side.
        declared_after: String,
        /// The declared edge's dependency side.
        declared_before: String,
        /// The declared edge's own detail text.
        detail: String,
        /// The rule that overrode it.
        by: EdgeWinnerDto,
    },
    /// See [`Rationale::AnyOfNoCandidate`].
    AnyOfNoCandidate,
    /// See [`Rationale::ForceAnyOfCandidateAcceptingCycle`].
    ForceAnyOfCandidateAcceptingCycle,
    /// See [`Rationale::AnyOfChosenAlreadyBefore`].
    AnyOfChosenAlreadyBefore,
    /// See [`Rationale::AnyOfAlternativeCandidate`].
    AnyOfAlternativeCandidate,
    /// See [`Rationale::AnyOfUniqueMaxDependents`].
    AnyOfUniqueMaxDependents,
    /// See [`Rationale::AnyOfSmallestId`].
    AnyOfSmallestId,

    // -- rules.rs --
    /// See [`Rationale::RuleOverruledLongerCycle`].
    #[serde(rename_all = "camelCase")]
    RuleOverruledLongerCycle {
        /// The rule's dependent side.
        after: String,
        /// The rule's dependency side.
        before: String,
        /// Where the rule came from.
        origin: RuleOriginDto,
        /// Whether the rule itself overrides a declaration.
        overrides_declared: bool,
    },
    /// See [`Rationale::RuleOverruledByWinner`].
    #[serde(rename_all = "camelCase")]
    RuleOverruledByWinner {
        /// The rule's dependent side.
        after: String,
        /// The rule's dependency side.
        before: String,
        /// Where the rule came from.
        origin: RuleOriginDto,
        /// Whether the rule itself overrides a declaration.
        overrides_declared: bool,
        /// The edge that won.
        winner: EdgeWinnerDto,
    },
    /// See [`Rationale::ReorderOverridingRuleWinner`].
    ReorderOverridingRuleWinner,
    /// See [`Rationale::PromoteRuleDespiteOverruled`].
    PromoteRuleDespiteOverruled,
    /// See [`Rationale::PlacementOverruled`].
    #[serde(rename_all = "camelCase")]
    PlacementOverruled {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it was pinned to.
        placement: PlacementDto,
        /// The edge that overruled the pin.
        by: EdgeWinnerDto,
    },
    /// See [`Rationale::PromotePlacementDespiteOverruled`].
    #[serde(rename_all = "camelCase")]
    PromotePlacementDespiteOverruled {
        /// Which tier the pin is on.
        placement: PlacementDto,
    },
    /// See [`Rationale::DropRuleOverrulingPlacement`].
    #[serde(rename_all = "camelCase")]
    DropRuleOverrulingPlacement {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it's pinned to.
        placement: PlacementDto,
    },
    /// See [`Rationale::PlacementQuestioned`].
    #[serde(rename_all = "camelCase")]
    PlacementQuestioned {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it's pinned to.
        placement: PlacementDto,
        /// The advisory relation's own kind.
        relation_kind: EdgeKindDto,
        /// The advisory relation's own detail text.
        relation_detail: String,
    },
    /// See [`Rationale::EnforceRelationAsUserDecision`].
    EnforceRelationAsUserDecision,
    /// See [`Rationale::PlacementOrderingOverridden`].
    #[serde(rename_all = "camelCase")]
    PlacementOrderingOverridden {
        /// The mod forced across the pin's own extreme edge.
        mod_id: String,
        /// The pin it could not be sorted around.
        pinned: String,
        /// Which tier the pin occupies.
        placement: PlacementDto,
        /// The accepted edge that forced this ordering.
        by: EdgeWinnerDto,
    },
    /// See [`Rationale::PlacementPromotesDependents`].
    #[serde(rename_all = "camelCase")]
    PlacementPromotesDependents {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it's pinned to.
        placement: PlacementDto,
        /// Every other mod attributed to this pin's own promotion, sorted.
        promoted: Vec<String>,
    },
    /// See [`Rationale::TagInferredSignalCount`].
    TagInferredSignalCount {
        /// The matched-signal count.
        count: usize,
    },
    /// See [`Rationale::RejectInferredTag`].
    RejectInferredTag,

    // -- defs.rs --
    /// See [`Rationale::ForceDefOverrideWinner`].
    ForceDefOverrideWinner,
    /// See [`Rationale::DefOverrideWinnerDeclaresRelation`].
    DefOverrideWinnerDeclaresRelation,
    /// See [`Rationale::DefOverrideSameAuthor`].
    DefOverrideSameAuthor,
    /// See [`Rationale::DefOverrideLoneNonVanillaOwner`].
    DefOverrideLoneNonVanillaOwner,
    /// See [`Rationale::DefOverrideShadowsFramework`].
    DefOverrideShadowsFramework,
    /// See [`Rationale::KeepCurrentWinner`].
    KeepCurrentWinner,
    /// See [`Rationale::DefOverrideUnexplained`].
    DefOverrideUnexplained,
    /// See [`Rationale::MergeUnexplainedDefOverride`].
    MergeUnexplainedDefOverride,
    /// See [`Rationale::MergeShadowsFrameworkFieldByField`].
    MergeShadowsFrameworkFieldByField,
    /// See [`Rationale::ForcePatchCollisionWinner`].
    ForcePatchCollisionWinner,
    /// See [`Rationale::PatchCollisionAdditive`].
    PatchCollisionAdditive,
    /// See [`Rationale::PatchCollisionContested`].
    PatchCollisionContested,
    /// See [`Rationale::PatchCollisionWinnerDeclaresRelation`].
    #[serde(rename_all = "camelCase")]
    PatchCollisionWinnerDeclaresRelation {
        /// The mod whose operation runs last.
        winner: String,
        /// Every other mod patching the field, in load order.
        others: Vec<String>,
        /// The patched field's path under the def.
        field: String,
    },
    /// See [`Rationale::MergeContestedPatchCollision`].
    MergeContestedPatchCollision,
    /// See [`Rationale::PatchWillFailRemovedBy`].
    #[serde(rename_all = "camelCase")]
    PatchWillFailRemovedBy {
        /// The mod whose operation removed the targeted node.
        remover: String,
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The failing operation's own leaf xpath (or top-level identity).
        xpath: String,
    },
    /// See [`Rationale::ReorderBeforeRemover`].
    #[serde(rename_all = "camelCase")]
    ReorderBeforeRemover {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The mod whose operation removed the targeted node.
        remover: String,
    },
    /// See [`Rationale::PatchWillFailNotYetInjected`].
    #[serde(rename_all = "camelCase")]
    PatchWillFailNotYetInjected {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The mod whose own operation injects the targeted node.
        injector: String,
        /// The failing operation's own leaf xpath (or top-level identity).
        xpath: String,
    },
    /// See [`Rationale::ReorderAfterInjector`].
    #[serde(rename_all = "camelCase")]
    ReorderAfterInjector {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The mod whose own operation injects the targeted node.
        injector: String,
    },
    /// See [`Rationale::PatchWillFailDeadTarget`].
    #[serde(rename_all = "camelCase")]
    PatchWillFailDeadTarget {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The failing operation's own leaf xpath (or top-level identity).
        xpath: String,
    },
    /// See [`Rationale::PatchWillFailUnknownCause`].
    #[serde(rename_all = "camelCase")]
    PatchWillFailUnknownCause {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The failing operation's own leaf xpath (or top-level identity).
        xpath: String,
    },
    /// See [`Rationale::BrokenInheritance`].
    BrokenInheritance,
    /// See [`Rationale::DanglingDefReference`].
    #[serde(rename_all = "camelCase")]
    DanglingDefReference {
        /// Why the referenced name never resolves.
        cause: DanglingCauseDto,
        /// Whether a `SoundDef`-fallback caveat applies.
        likely_sound: bool,
    },
    /// See [`Rationale::DiscardedAdditionDeliberate`].
    #[serde(rename_all = "camelCase")]
    DiscardedAdditionDeliberate {
        /// The mod whose `Replace` discards the addition.
        replacer: String,
        /// The mod whose addition is discarded.
        adder: String,
        /// The path `replacer`'s own `Replace` targets.
        path: String,
        /// The path `adder`'s own addition wrote into.
        adder_path: String,
    },

    // -- assets.rs --
    /// See [`Rationale::ForceTextureWinner`].
    ForceTextureWinner,
    /// See [`Rationale::CopyTextureIntoMergeMod`].
    CopyTextureIntoMergeMod,
    /// See [`Rationale::TextureOverrideCosmetic`].
    TextureOverrideCosmetic,
    /// See [`Rationale::ForceTemplateRegistrationWinner`].
    ForceTemplateRegistrationWinner,
    /// See [`Rationale::DuplicateTemplateNameExplanation`].
    #[serde(rename_all = "camelCase")]
    DuplicateTemplateNameExplanation {
        /// How many mods register this template name.
        owner_count: usize,
    },
    /// See [`Rationale::KeyedTranslationCollision`].
    #[serde(rename_all = "camelCase")]
    KeyedTranslationCollision {
        /// How many keys the two mods collide over.
        key_count: usize,
    },
    /// See [`Rationale::SoundOverrideCosmetic`].
    SoundOverrideCosmetic,
    /// See [`Rationale::ForceSoundWinner`].
    ForceSoundWinner,
    /// See [`Rationale::MissingTexturePath`].
    MissingTexturePath,
    /// See [`Rationale::UndecodableTexture`].
    UndecodableTexture,

    // -- mods.rs --
    /// See [`Rationale::ContributesNothing`].
    #[serde(rename_all = "camelCase")]
    ContributesNothing {
        /// The mod that contributes nothing.
        mod_id: String,
    },
    /// See [`Rationale::UndeclaredTypeDependency`].
    #[serde(rename_all = "camelCase")]
    UndeclaredTypeDependency {
        /// The mod using the type.
        user: String,
        /// The mod whose assembly declares the type.
        provider: String,
    },
    /// See [`Rationale::PinRelationExplicitly`].
    PinRelationExplicitly,
    /// See [`Rationale::RuntimePatchCollisionNoLastPatcher`].
    #[serde(rename_all = "camelCase")]
    RuntimePatchCollisionNoLastPatcher {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
    },
    /// See [`Rationale::RuntimePatchCollisionLastPatcher`].
    #[serde(rename_all = "camelCase")]
    RuntimePatchCollisionLastPatcher {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// How many mods patch this target.
        owner_count: usize,
        /// The mod whose patch runs last today.
        last: String,
    },
    /// See [`Rationale::ForceRuntimePatchLastWinner`].
    ForceRuntimePatchLastWinner,
    /// See [`Rationale::TranspilerCollision`].
    #[serde(rename_all = "camelCase")]
    TranspilerCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// How many mods rewrite this method's IL.
        owner_count: usize,
    },
    /// See [`Rationale::DuplicateAssemblyFirstLoadedWins`].
    DuplicateAssemblyFirstLoadedWins,
    /// See [`Rationale::RemoveDuplicateAssemblyCopy`].
    RemoveDuplicateAssemblyCopy,
    /// See [`Rationale::LikelyDuplicateMod`].
    LikelyDuplicateMod,
    /// See [`Rationale::RemoveLikelyDuplicateModA`].
    RemoveLikelyDuplicateModA,
    /// See [`Rationale::RemoveLikelyDuplicateModB`].
    RemoveLikelyDuplicateModB,
    /// See [`Rationale::MissingModNotInstalled`].
    MissingModNotInstalled,
    /// See [`Rationale::KeepMissingModId`].
    KeepMissingModId,
    /// See [`Rationale::MissingDependencyNotEnforced`].
    MissingDependencyNotEnforced,
    /// See [`Rationale::IncompatiblePair`].
    IncompatiblePair,
    /// See [`Rationale::RemoveIncompatibleModA`].
    RemoveIncompatibleModA,
    /// See [`Rationale::RemoveIncompatibleModB`].
    RemoveIncompatibleModB,
    /// See [`Rationale::UnsupportedVersionOftenWorks`].
    UnsupportedVersionOftenWorks,
    /// See [`Rationale::UndeclaredHardDependencyHonored`].
    UndeclaredHardDependencyHonored,
    /// See [`Rationale::PinRelationExplicitlyAnyway`].
    PinRelationExplicitlyAnyway,
    /// See [`Rationale::LazyReferenceViolatedJitResolved`].
    LazyReferenceViolatedJitResolved,
    /// See [`Rationale::NearMissModReference`].
    #[serde(rename_all = "camelCase")]
    NearMissModReference {
        /// Which kind of reference this is. Named `referenceKind`, not
        /// `kind` — this enum's own `#[serde(tag = "kind")]`
        /// discriminant already claims that field name at the wire
        /// level.
        reference_kind: ModReferenceKindDto,
        /// The name or id as written.
        written: String,
        /// The active mod it closely resembles.
        candidate_name: String,
    },

    // -- domain::resolution.rs --
    /// See [`Rationale::MergeCompleteNothingToMerge`].
    MergeCompleteNothingToMerge,
    /// See [`Rationale::MergeLeadDefOverrideCombine`].
    MergeLeadDefOverrideCombine,
    /// See [`Rationale::MergePromotedPatchCollisionKeepsBoth`].
    MergePromotedPatchCollisionKeepsBoth,
    /// See [`Rationale::KeepLoadOrderWinner`].
    KeepLoadOrderWinner,
    /// See [`Rationale::MergeStructuralGuard`].
    #[serde(rename_all = "camelCase")]
    MergeStructuralGuard {
        /// The guard's own field name.
        field: String,
    },
    /// See [`Rationale::MergeFieldsNeedChoice`].
    #[serde(rename_all = "camelCase")]
    MergeFieldsNeedChoice {
        /// How many fields still need a choice.
        unresolved: usize,
    },
    /// See [`Rationale::IdenticalCopiesOrderIrrelevant`].
    IdenticalCopiesOrderIrrelevant,

    // -- ledger/scoped.rs --
    /// See [`Rationale::NotAddressedByPatch`].
    NotAddressedByPatch,
}

impl From<&Rationale> for RationaleDto {
    fn from(value: &Rationale) -> Self {
        match value {
            Rationale::EdgeDroppedInferred {
                kind,
                after,
                before,
            } => Self::EdgeDroppedInferred {
                edge_kind: (*kind).into(),
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
            },
            Rationale::EdgeDroppedCosmetic => Self::EdgeDroppedCosmetic,
            Rationale::KeepEdgeInCycle => Self::KeepEdgeInCycle,
            Rationale::ReorderOverridingEdgeWinner => Self::ReorderOverridingEdgeWinner,
            Rationale::EdgeDroppedWithWinner {
                after,
                before,
                winner,
            } => Self::EdgeDroppedWithWinner {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                winner: winner.into(),
            },
            Rationale::EdgeDroppedSoftOrAwareness => Self::EdgeDroppedSoftOrAwareness,
            Rationale::EdgeDroppedDeclared => Self::EdgeDroppedDeclared,
            Rationale::EdgeDroppedHard => Self::EdgeDroppedHard,
            Rationale::RemoveDependentModBrokenOrder => Self::RemoveDependentModBrokenOrder,
            Rationale::DeclarationQuestioned {
                declared_after,
                declared_before,
                declared_layer,
                declared_detail,
                relation_kind,
                relation_detail,
            } => Self::DeclarationQuestioned {
                declared_after: declared_after.as_str().to_string(),
                declared_before: declared_before.as_str().to_string(),
                declared_layer: (*declared_layer).into(),
                declared_detail: declared_detail.clone(),
                relation_kind: (*relation_kind).into(),
                relation_detail: relation_detail.clone(),
            },
            Rationale::ReorderOppositeOfRelation => Self::ReorderOppositeOfRelation,
            Rationale::DeclarationOverridden {
                declared_after,
                declared_before,
                detail,
                by,
            } => Self::DeclarationOverridden {
                declared_after: declared_after.as_str().to_string(),
                declared_before: declared_before.as_str().to_string(),
                detail: detail.clone(),
                by: by.into(),
            },
            Rationale::AnyOfNoCandidate => Self::AnyOfNoCandidate,
            Rationale::ForceAnyOfCandidateAcceptingCycle => Self::ForceAnyOfCandidateAcceptingCycle,
            Rationale::AnyOfChosenAlreadyBefore => Self::AnyOfChosenAlreadyBefore,
            Rationale::AnyOfAlternativeCandidate => Self::AnyOfAlternativeCandidate,
            Rationale::AnyOfUniqueMaxDependents => Self::AnyOfUniqueMaxDependents,
            Rationale::AnyOfSmallestId => Self::AnyOfSmallestId,
            Rationale::RuleOverruledLongerCycle {
                after,
                before,
                origin,
                overrides_declared,
            } => Self::RuleOverruledLongerCycle {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                origin: (*origin).into(),
                overrides_declared: *overrides_declared,
            },
            Rationale::RuleOverruledByWinner {
                after,
                before,
                origin,
                overrides_declared,
                winner,
            } => Self::RuleOverruledByWinner {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                origin: (*origin).into(),
                overrides_declared: *overrides_declared,
                winner: winner.into(),
            },
            Rationale::ReorderOverridingRuleWinner => Self::ReorderOverridingRuleWinner,
            Rationale::PromoteRuleDespiteOverruled => Self::PromoteRuleDespiteOverruled,
            Rationale::PlacementOverruled {
                mod_id,
                placement,
                by,
            } => Self::PlacementOverruled {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                by: by.into(),
            },
            Rationale::PromotePlacementDespiteOverruled { placement } => {
                Self::PromotePlacementDespiteOverruled {
                    placement: (*placement).into(),
                }
            }
            Rationale::DropRuleOverrulingPlacement { mod_id, placement } => {
                Self::DropRuleOverrulingPlacement {
                    mod_id: mod_id.as_str().to_string(),
                    placement: (*placement).into(),
                }
            }
            Rationale::PlacementQuestioned {
                mod_id,
                placement,
                relation_kind,
                relation_detail,
            } => Self::PlacementQuestioned {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                relation_kind: (*relation_kind).into(),
                relation_detail: relation_detail.clone(),
            },
            Rationale::EnforceRelationAsUserDecision => Self::EnforceRelationAsUserDecision,
            Rationale::PlacementOrderingOverridden {
                mod_id,
                pinned,
                placement,
                by,
            } => Self::PlacementOrderingOverridden {
                mod_id: mod_id.as_str().to_string(),
                pinned: pinned.as_str().to_string(),
                placement: (*placement).into(),
                by: by.into(),
            },
            Rationale::PlacementPromotesDependents {
                mod_id,
                placement,
                promoted,
            } => Self::PlacementPromotesDependents {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                promoted: promoted.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Rationale::TagInferredSignalCount { count } => {
                Self::TagInferredSignalCount { count: *count }
            }
            Rationale::RejectInferredTag => Self::RejectInferredTag,
            Rationale::ForceDefOverrideWinner => Self::ForceDefOverrideWinner,
            Rationale::DefOverrideWinnerDeclaresRelation => Self::DefOverrideWinnerDeclaresRelation,
            Rationale::DefOverrideSameAuthor => Self::DefOverrideSameAuthor,
            Rationale::DefOverrideLoneNonVanillaOwner => Self::DefOverrideLoneNonVanillaOwner,
            Rationale::DefOverrideShadowsFramework => Self::DefOverrideShadowsFramework,
            Rationale::KeepCurrentWinner => Self::KeepCurrentWinner,
            Rationale::DefOverrideUnexplained => Self::DefOverrideUnexplained,
            Rationale::MergeUnexplainedDefOverride => Self::MergeUnexplainedDefOverride,
            Rationale::MergeShadowsFrameworkFieldByField => Self::MergeShadowsFrameworkFieldByField,
            Rationale::ForcePatchCollisionWinner => Self::ForcePatchCollisionWinner,
            Rationale::PatchCollisionAdditive => Self::PatchCollisionAdditive,
            Rationale::PatchCollisionContested => Self::PatchCollisionContested,
            Rationale::PatchCollisionWinnerDeclaresRelation {
                winner,
                others,
                field,
            } => Self::PatchCollisionWinnerDeclaresRelation {
                winner: winner.as_str().to_string(),
                others: others.iter().map(|id| id.as_str().to_string()).collect(),
                field: field.clone(),
            },
            Rationale::MergeContestedPatchCollision => Self::MergeContestedPatchCollision,
            Rationale::PatchWillFailRemovedBy {
                remover,
                mod_id,
                xpath,
            } => Self::PatchWillFailRemovedBy {
                remover: remover.as_str().to_string(),
                mod_id: mod_id.as_str().to_string(),
                xpath: xpath.clone(),
            },
            Rationale::ReorderBeforeRemover { mod_id, remover } => Self::ReorderBeforeRemover {
                mod_id: mod_id.as_str().to_string(),
                remover: remover.as_str().to_string(),
            },
            Rationale::PatchWillFailNotYetInjected {
                mod_id,
                injector,
                xpath,
            } => Self::PatchWillFailNotYetInjected {
                mod_id: mod_id.as_str().to_string(),
                injector: injector.as_str().to_string(),
                xpath: xpath.clone(),
            },
            Rationale::ReorderAfterInjector { mod_id, injector } => Self::ReorderAfterInjector {
                mod_id: mod_id.as_str().to_string(),
                injector: injector.as_str().to_string(),
            },
            Rationale::PatchWillFailDeadTarget { mod_id, xpath } => Self::PatchWillFailDeadTarget {
                mod_id: mod_id.as_str().to_string(),
                xpath: xpath.clone(),
            },
            Rationale::PatchWillFailUnknownCause { mod_id, xpath } => {
                Self::PatchWillFailUnknownCause {
                    mod_id: mod_id.as_str().to_string(),
                    xpath: xpath.clone(),
                }
            }
            Rationale::BrokenInheritance => Self::BrokenInheritance,
            Rationale::DanglingDefReference {
                cause,
                likely_sound,
            } => Self::DanglingDefReference {
                cause: cause.into(),
                likely_sound: *likely_sound,
            },
            Rationale::DiscardedAdditionDeliberate {
                replacer,
                adder,
                path,
                adder_path,
            } => Self::DiscardedAdditionDeliberate {
                replacer: replacer.as_str().to_string(),
                adder: adder.as_str().to_string(),
                path: path.clone(),
                adder_path: adder_path.clone(),
            },
            Rationale::ForceTextureWinner => Self::ForceTextureWinner,
            Rationale::CopyTextureIntoMergeMod => Self::CopyTextureIntoMergeMod,
            Rationale::TextureOverrideCosmetic => Self::TextureOverrideCosmetic,
            Rationale::ForceTemplateRegistrationWinner => Self::ForceTemplateRegistrationWinner,
            Rationale::DuplicateTemplateNameExplanation { owner_count } => {
                Self::DuplicateTemplateNameExplanation {
                    owner_count: *owner_count,
                }
            }
            Rationale::KeyedTranslationCollision { key_count } => Self::KeyedTranslationCollision {
                key_count: *key_count,
            },
            Rationale::SoundOverrideCosmetic => Self::SoundOverrideCosmetic,
            Rationale::ForceSoundWinner => Self::ForceSoundWinner,
            Rationale::MissingTexturePath => Self::MissingTexturePath,
            Rationale::UndecodableTexture => Self::UndecodableTexture,
            Rationale::ContributesNothing { mod_id } => Self::ContributesNothing {
                mod_id: mod_id.as_str().to_string(),
            },
            Rationale::UndeclaredTypeDependency { user, provider } => {
                Self::UndeclaredTypeDependency {
                    user: user.as_str().to_string(),
                    provider: provider.as_str().to_string(),
                }
            }
            Rationale::PinRelationExplicitly => Self::PinRelationExplicitly,
            Rationale::RuntimePatchCollisionNoLastPatcher {
                target_type,
                target_method,
            } => Self::RuntimePatchCollisionNoLastPatcher {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
            },
            Rationale::RuntimePatchCollisionLastPatcher {
                target_type,
                target_method,
                owner_count,
                last,
            } => Self::RuntimePatchCollisionLastPatcher {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owner_count: *owner_count,
                last: last.as_str().to_string(),
            },
            Rationale::ForceRuntimePatchLastWinner => Self::ForceRuntimePatchLastWinner,
            Rationale::TranspilerCollision {
                target_type,
                target_method,
                owner_count,
            } => Self::TranspilerCollision {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owner_count: *owner_count,
            },
            Rationale::DuplicateAssemblyFirstLoadedWins => Self::DuplicateAssemblyFirstLoadedWins,
            Rationale::RemoveDuplicateAssemblyCopy => Self::RemoveDuplicateAssemblyCopy,
            Rationale::LikelyDuplicateMod => Self::LikelyDuplicateMod,
            Rationale::RemoveLikelyDuplicateModA => Self::RemoveLikelyDuplicateModA,
            Rationale::RemoveLikelyDuplicateModB => Self::RemoveLikelyDuplicateModB,
            Rationale::MissingModNotInstalled => Self::MissingModNotInstalled,
            Rationale::KeepMissingModId => Self::KeepMissingModId,
            Rationale::MissingDependencyNotEnforced => Self::MissingDependencyNotEnforced,
            Rationale::IncompatiblePair => Self::IncompatiblePair,
            Rationale::RemoveIncompatibleModA => Self::RemoveIncompatibleModA,
            Rationale::RemoveIncompatibleModB => Self::RemoveIncompatibleModB,
            Rationale::UnsupportedVersionOftenWorks => Self::UnsupportedVersionOftenWorks,
            Rationale::UndeclaredHardDependencyHonored => Self::UndeclaredHardDependencyHonored,
            Rationale::PinRelationExplicitlyAnyway => Self::PinRelationExplicitlyAnyway,
            Rationale::LazyReferenceViolatedJitResolved => Self::LazyReferenceViolatedJitResolved,
            Rationale::NearMissModReference {
                kind,
                written,
                candidate_name,
            } => Self::NearMissModReference {
                reference_kind: (*kind).into(),
                written: written.clone(),
                candidate_name: candidate_name.clone(),
            },
            Rationale::MergeCompleteNothingToMerge => Self::MergeCompleteNothingToMerge,
            Rationale::MergeLeadDefOverrideCombine => Self::MergeLeadDefOverrideCombine,
            Rationale::MergePromotedPatchCollisionKeepsBoth => {
                Self::MergePromotedPatchCollisionKeepsBoth
            }
            Rationale::KeepLoadOrderWinner => Self::KeepLoadOrderWinner,
            Rationale::MergeStructuralGuard { field } => Self::MergeStructuralGuard {
                field: field.clone(),
            },
            Rationale::MergeFieldsNeedChoice { unresolved } => Self::MergeFieldsNeedChoice {
                unresolved: *unresolved,
            },
            Rationale::IdenticalCopiesOrderIrrelevant => Self::IdenticalCopiesOrderIrrelevant,
            Rationale::NotAddressedByPatch => Self::NotAddressedByPatch,
        }
    }
}
