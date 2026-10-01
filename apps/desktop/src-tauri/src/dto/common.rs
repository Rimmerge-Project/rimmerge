//! Small, closed-vocabulary DTOs shared across the command surface:
//! mirrors of `rim-analyzer`/`rim-resolve` enums that cross IPC in both
//! directions (so each needs an exact, exhaustive mapping both ways,
//! never a fallback arm — a new domain variant must fail to compile
//! here, not silently drop through).

use rim_analyzer::domain::{EdgeKind, EdgeStrength, Selector, Source};
use rim_resolve::domain::{Placement, RuleOrigin, TagMode};
use rim_resolve::sort::Tier;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Mirrors [`EdgeKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EdgeKindDto {
    /// See [`EdgeKind::AssemblyRef`].
    AssemblyRef,
    /// See [`EdgeKind::ForceLoadAfter`].
    ForceLoadAfter,
    /// See [`EdgeKind::ForceLoadBefore`].
    ForceLoadBefore,
    /// See [`EdgeKind::LoadAfter`].
    LoadAfter,
    /// See [`EdgeKind::LoadBefore`].
    LoadBefore,
    /// See [`EdgeKind::ModDependency`].
    ModDependency,
    /// See [`EdgeKind::FindMod`].
    FindMod,
    /// See [`EdgeKind::IfModActive`].
    IfModActive,
    /// See [`EdgeKind::PatchTargetsDef`].
    PatchTargetsDef,
    /// See [`EdgeKind::MayRequire`].
    MayRequire,
    /// See [`EdgeKind::PatchInjectedNode`].
    PatchInjectedNode,
    /// See [`EdgeKind::AssemblyVersionPrecedence`].
    AssemblyVersionPrecedence,
    /// See [`EdgeKind::UsesType`].
    UsesType,
    /// See [`EdgeKind::ParentTemplate`].
    ParentTemplate,
    /// See [`EdgeKind::PatchRemovedNode`].
    PatchRemovedNode,
    /// See [`EdgeKind::RetextureAfterOwner`].
    RetextureAfterOwner,
    /// See [`EdgeKind::DefOverrideAfterOrigin`].
    DefOverrideAfterOrigin,
    /// See [`EdgeKind::PatchSelectsInjectedNode`].
    PatchSelectsInjectedNode,
    /// See [`EdgeKind::PatchInvalidatesPredicate`].
    PatchInvalidatesPredicate,
    /// See [`EdgeKind::PatchRemovedNodeCosmetic`].
    PatchRemovedNodeCosmetic,
    /// See [`EdgeKind::ReplaceDiscardsAddition`].
    ReplaceDiscardsAddition,
}

impl From<EdgeKind> for EdgeKindDto {
    fn from(value: EdgeKind) -> Self {
        match value {
            EdgeKind::AssemblyRef => Self::AssemblyRef,
            EdgeKind::ForceLoadAfter => Self::ForceLoadAfter,
            EdgeKind::ForceLoadBefore => Self::ForceLoadBefore,
            EdgeKind::LoadAfter => Self::LoadAfter,
            EdgeKind::LoadBefore => Self::LoadBefore,
            EdgeKind::ModDependency => Self::ModDependency,
            EdgeKind::FindMod => Self::FindMod,
            EdgeKind::IfModActive => Self::IfModActive,
            EdgeKind::PatchTargetsDef => Self::PatchTargetsDef,
            EdgeKind::MayRequire => Self::MayRequire,
            EdgeKind::PatchInjectedNode => Self::PatchInjectedNode,
            EdgeKind::AssemblyVersionPrecedence => Self::AssemblyVersionPrecedence,
            EdgeKind::UsesType => Self::UsesType,
            EdgeKind::ParentTemplate => Self::ParentTemplate,
            EdgeKind::PatchRemovedNode => Self::PatchRemovedNode,
            EdgeKind::RetextureAfterOwner => Self::RetextureAfterOwner,
            EdgeKind::DefOverrideAfterOrigin => Self::DefOverrideAfterOrigin,
            EdgeKind::PatchSelectsInjectedNode => Self::PatchSelectsInjectedNode,
            EdgeKind::PatchInvalidatesPredicate => Self::PatchInvalidatesPredicate,
            EdgeKind::PatchRemovedNodeCosmetic => Self::PatchRemovedNodeCosmetic,
            EdgeKind::ReplaceDiscardsAddition => Self::ReplaceDiscardsAddition,
        }
    }
}

impl From<EdgeKindDto> for EdgeKind {
    fn from(value: EdgeKindDto) -> Self {
        match value {
            EdgeKindDto::AssemblyRef => Self::AssemblyRef,
            EdgeKindDto::ForceLoadAfter => Self::ForceLoadAfter,
            EdgeKindDto::ForceLoadBefore => Self::ForceLoadBefore,
            EdgeKindDto::LoadAfter => Self::LoadAfter,
            EdgeKindDto::LoadBefore => Self::LoadBefore,
            EdgeKindDto::ModDependency => Self::ModDependency,
            EdgeKindDto::FindMod => Self::FindMod,
            EdgeKindDto::IfModActive => Self::IfModActive,
            EdgeKindDto::PatchTargetsDef => Self::PatchTargetsDef,
            EdgeKindDto::MayRequire => Self::MayRequire,
            EdgeKindDto::PatchInjectedNode => Self::PatchInjectedNode,
            EdgeKindDto::AssemblyVersionPrecedence => Self::AssemblyVersionPrecedence,
            EdgeKindDto::UsesType => Self::UsesType,
            EdgeKindDto::ParentTemplate => Self::ParentTemplate,
            EdgeKindDto::PatchRemovedNode => Self::PatchRemovedNode,
            EdgeKindDto::RetextureAfterOwner => Self::RetextureAfterOwner,
            EdgeKindDto::DefOverrideAfterOrigin => Self::DefOverrideAfterOrigin,
            EdgeKindDto::PatchSelectsInjectedNode => Self::PatchSelectsInjectedNode,
            EdgeKindDto::PatchInvalidatesPredicate => Self::PatchInvalidatesPredicate,
            EdgeKindDto::PatchRemovedNodeCosmetic => Self::PatchRemovedNodeCosmetic,
            EdgeKindDto::ReplaceDiscardsAddition => Self::ReplaceDiscardsAddition,
        }
    }
}

/// Mirrors [`EdgeStrength`]. Display-only: never received from the
/// frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EdgeStrengthDto {
    /// See [`EdgeStrength::Hard`].
    Hard,
    /// See [`EdgeStrength::Declared`].
    Declared,
    /// See [`EdgeStrength::Soft`].
    Soft,
    /// See [`EdgeStrength::Awareness`].
    Awareness,
    /// See [`EdgeStrength::Inferred`]. A dedicated variant rather than folded
    /// into `Awareness` on the wire, which would mismatch `ledger::suggest`'s
    /// own dedicated confidence-70 rationale text for a dropped one.
    Inferred,
}

impl From<EdgeStrength> for EdgeStrengthDto {
    fn from(value: EdgeStrength) -> Self {
        match value {
            EdgeStrength::Hard => Self::Hard,
            EdgeStrength::Declared => Self::Declared,
            EdgeStrength::Soft => Self::Soft,
            EdgeStrength::Awareness => Self::Awareness,
            EdgeStrength::Inferred => Self::Inferred,
        }
    }
}

/// Mirrors [`Source`]. Display-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SourceDto {
    /// See [`Source::Core`].
    Core,
    /// See [`Source::Dlc`].
    Dlc,
    /// See [`Source::Local`].
    Local,
    /// See [`Source::Workshop`].
    Workshop,
}

impl From<Source> for SourceDto {
    fn from(value: Source) -> Self {
        match value {
            Source::Core => Self::Core,
            Source::Dlc => Self::Dlc,
            Source::Local => Self::Local,
            Source::Workshop => Self::Workshop,
        }
    }
}

impl From<SourceDto> for Source {
    fn from(value: SourceDto) -> Self {
        match value {
            SourceDto::Core => Self::Core,
            SourceDto::Dlc => Self::Dlc,
            SourceDto::Local => Self::Local,
            SourceDto::Workshop => Self::Workshop,
        }
    }
}

/// Mirrors [`RuleOrigin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RuleOriginDto {
    /// See [`RuleOrigin::UserDecision`].
    UserDecision,
    /// See [`RuleOrigin::RimSortUser`].
    RimSortUser,
    /// See [`RuleOrigin::RimSortCommunity`].
    RimSortCommunity,
    /// See [`RuleOrigin::SteamDb`].
    SteamDb,
}

impl From<RuleOrigin> for RuleOriginDto {
    fn from(value: RuleOrigin) -> Self {
        match value {
            RuleOrigin::UserDecision => Self::UserDecision,
            RuleOrigin::RimSortUser => Self::RimSortUser,
            RuleOrigin::RimSortCommunity => Self::RimSortCommunity,
            RuleOrigin::SteamDb => Self::SteamDb,
        }
    }
}

impl From<RuleOriginDto> for RuleOrigin {
    fn from(value: RuleOriginDto) -> Self {
        match value {
            RuleOriginDto::UserDecision => Self::UserDecision,
            RuleOriginDto::RimSortUser => Self::RimSortUser,
            RuleOriginDto::RimSortCommunity => Self::RimSortCommunity,
            RuleOriginDto::SteamDb => Self::SteamDb,
        }
    }
}

/// Mirrors [`Placement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PlacementDto {
    /// See [`Placement::Top`].
    Top,
    /// See [`Placement::Bottom`].
    Bottom,
}

impl From<Placement> for PlacementDto {
    fn from(value: Placement) -> Self {
        match value {
            Placement::Top => Self::Top,
            Placement::Bottom => Self::Bottom,
        }
    }
}

impl From<PlacementDto> for Placement {
    fn from(value: PlacementDto) -> Self {
        match value {
            PlacementDto::Top => Self::Top,
            PlacementDto::Bottom => Self::Bottom,
        }
    }
}

/// Mirrors [`TagMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TagModeDto {
    /// See [`TagMode::Add`].
    Add,
    /// See [`TagMode::Remove`].
    Remove,
}

impl From<TagMode> for TagModeDto {
    fn from(value: TagMode) -> Self {
        match value {
            TagMode::Add => Self::Add,
            TagMode::Remove => Self::Remove,
        }
    }
}

impl From<TagModeDto> for TagMode {
    fn from(value: TagModeDto) -> Self {
        match value {
            TagModeDto::Add => Self::Add,
            TagModeDto::Remove => Self::Remove,
        }
    }
}

/// Mirrors [`rim_resolve::domain::OrderSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum OrderSourceDto {
    /// See [`rim_resolve::domain::OrderSource::Current`].
    Current,
    /// See [`rim_resolve::domain::OrderSource::Suggested`].
    Suggested,
}

impl From<rim_resolve::domain::OrderSource> for OrderSourceDto {
    fn from(value: rim_resolve::domain::OrderSource) -> Self {
        match value {
            rim_resolve::domain::OrderSource::Current => Self::Current,
            rim_resolve::domain::OrderSource::Suggested => Self::Suggested,
        }
    }
}

impl From<OrderSourceDto> for rim_resolve::domain::OrderSource {
    fn from(value: OrderSourceDto) -> Self {
        match value {
            OrderSourceDto::Current => Self::Current,
            OrderSourceDto::Suggested => Self::Suggested,
        }
    }
}

/// Mirrors [`rim_resolve::domain::ResolutionStatus`]. Display-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ResolutionStatusDto {
    /// See [`rim_resolve::domain::ResolutionStatus::Auto`].
    Auto,
    /// See [`rim_resolve::domain::ResolutionStatus::NeedsInput`].
    NeedsInput,
    /// See [`rim_resolve::domain::ResolutionStatus::UserOverridden`].
    UserOverridden,
}

impl From<rim_resolve::domain::ResolutionStatus> for ResolutionStatusDto {
    fn from(value: rim_resolve::domain::ResolutionStatus) -> Self {
        match value {
            rim_resolve::domain::ResolutionStatus::Auto => Self::Auto,
            rim_resolve::domain::ResolutionStatus::NeedsInput => Self::NeedsInput,
            rim_resolve::domain::ResolutionStatus::UserOverridden => Self::UserOverridden,
        }
    }
}

/// Mirrors [`Tier`]. Display-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TierDto {
    /// See [`Tier::Core`].
    Core,
    /// See [`Tier::Dlc`].
    Dlc,
    /// See [`Tier::Top`].
    Top,
    /// See [`Tier::Body`].
    Body,
    /// See [`Tier::Bottom`].
    Bottom,
}

impl From<Tier> for TierDto {
    fn from(value: Tier) -> Self {
        match value {
            Tier::Core => Self::Core,
            Tier::Dlc => Self::Dlc,
            Tier::Top => Self::Top,
            Tier::Body => Self::Body,
            Tier::Bottom => Self::Bottom,
        }
    }
}

/// Mirrors [`rim_analyzer::domain::EdgeStatus`]. Display-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EdgeStatusDto {
    /// See [`rim_analyzer::domain::EdgeStatus::Satisfied`].
    Satisfied,
    /// See [`rim_analyzer::domain::EdgeStatus::Violated`].
    Violated,
    /// See [`rim_analyzer::domain::EdgeStatus::Unevaluated`].
    Unevaluated,
}

impl From<rim_analyzer::domain::EdgeStatus> for EdgeStatusDto {
    fn from(value: rim_analyzer::domain::EdgeStatus) -> Self {
        match value {
            rim_analyzer::domain::EdgeStatus::Satisfied => Self::Satisfied,
            rim_analyzer::domain::EdgeStatus::Violated => Self::Violated,
            rim_analyzer::domain::EdgeStatus::Unevaluated => Self::Unevaluated,
        }
    }
}

/// The contested-def identity behind a `DefOverride`/`PatchCollision`
/// finding, and the target of `Action::PreferWinner`/`Action::Merge`.
/// Mirrors [`rim_resolve::domain::DefKey`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefKeyDto {
    /// The def's type tag, e.g. `ThingDef`.
    pub def_type: String,
    /// The def's `defName`.
    pub def_name: String,
}

impl From<rim_resolve::domain::DefKey> for DefKeyDto {
    fn from(value: rim_resolve::domain::DefKey) -> Self {
        Self {
            def_type: value.def_type,
            def_name: value.def_name,
        }
    }
}

impl From<DefKeyDto> for rim_resolve::domain::DefKey {
    fn from(value: DefKeyDto) -> Self {
        Self {
            def_type: value.def_type,
            def_name: value.def_name,
        }
    }
}

/// Mirrors [`Selector`]. Which attribute a patch xpath's def predicate
/// matched on — display-only, and (unlike `EdgeKindDto`/`SourceDto`/
/// `TierDto`/`RuleOriginDto` above) not yet rendered by any component;
/// added here so `FindingDto`/`VerifyDefTargetDto`/`VerifySkippedDto`
/// carry a code instead of a pre-rendered "defName"/"Name attribute"
/// string, per this crate's own "backend sends codes, frontend renders"
/// rule — a future UI surface for it adds a label helper then, not a
/// second string field now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SelectorDto {
    /// See [`Selector::DefName`].
    DefName,
    /// See [`Selector::NameAttr`].
    NameAttr,
}

impl From<Selector> for SelectorDto {
    fn from(value: Selector) -> Self {
        match value {
            Selector::DefName => Self::DefName,
            Selector::NameAttr => Self::NameAttr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-listed, not derived (`EdgeKind` has no enumerable-variants
    /// mechanism to derive from), and nothing forces it to track `EdgeKind`'s
    /// own variant count the way the `From` impls above are compiler-forced
    /// to — so a new `EdgeKind` variant needs an entry here too, the same
    /// discipline `arb_edge_kind` (`rim-resolve`'s `domain::finding`) and
    /// `ALL_EDGE_KINDS` (`apps/desktop/src/utils/edgeKind.test.ts`)
    /// already document on their own generators/lists.
    #[test]
    fn edge_kind_dto_round_trips_every_variant() {
        let kinds = [
            EdgeKind::AssemblyRef,
            EdgeKind::ForceLoadAfter,
            EdgeKind::ForceLoadBefore,
            EdgeKind::LoadAfter,
            EdgeKind::LoadBefore,
            EdgeKind::ModDependency,
            EdgeKind::FindMod,
            EdgeKind::IfModActive,
            EdgeKind::PatchTargetsDef,
            EdgeKind::MayRequire,
            EdgeKind::PatchInjectedNode,
            EdgeKind::AssemblyVersionPrecedence,
            EdgeKind::UsesType,
            EdgeKind::ParentTemplate,
            EdgeKind::PatchRemovedNode,
            EdgeKind::RetextureAfterOwner,
            EdgeKind::DefOverrideAfterOrigin,
            EdgeKind::PatchSelectsInjectedNode,
            EdgeKind::PatchInvalidatesPredicate,
            EdgeKind::PatchRemovedNodeCosmetic,
            EdgeKind::ReplaceDiscardsAddition,
        ];
        for kind in kinds {
            let dto: EdgeKindDto = kind.into();
            let back: EdgeKind = dto.into();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn source_dto_round_trips_every_variant() {
        for source in [Source::Core, Source::Dlc, Source::Local, Source::Workshop] {
            let dto: SourceDto = source.into();
            let back: Source = dto.into();
            assert_eq!(source, back);
        }
    }

    #[test]
    fn selector_dto_maps_every_variant() {
        assert_eq!(SelectorDto::from(Selector::DefName), SelectorDto::DefName);
        assert_eq!(SelectorDto::from(Selector::NameAttr), SelectorDto::NameAttr);
    }

    #[test]
    fn def_key_dto_maps_field_by_field() {
        let key = rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        };
        let dto: DefKeyDto = key.clone().into();
        assert_eq!(dto.def_type, "ThingDef");
        assert_eq!(dto.def_name, "Wall");
        let back: rim_resolve::domain::DefKey = dto.into();
        assert_eq!(key, back);
    }
}
