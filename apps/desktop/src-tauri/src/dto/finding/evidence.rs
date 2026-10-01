//! The finding itself: edge winners, patch failure causes, `FindingDto` and its title.

use rim_resolve::domain::Finding;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::suggestion::TagSignalDto;
use crate::dto::common::{
    DefKeyDto, EdgeKindDto, EdgeStrengthDto, PlacementDto, RuleOriginDto, SelectorDto,
};
use crate::dto::order::LayerDto;

/// The edge that overruled a [`Finding::EdgeDropped`]'s edge in a direct
/// two-mod contradiction. Mirrors [`rim_resolve::domain::EdgeWinner`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EdgeWinnerDto {
    /// The winning edge's dependent side.
    pub after: String,
    /// The winning edge's dependency side.
    pub before: String,
    /// Which layer the winning edge belongs to.
    pub layer: LayerDto,
    /// A human-readable description of the winning edge.
    pub detail: String,
}

impl From<&rim_resolve::domain::EdgeWinner> for EdgeWinnerDto {
    fn from(value: &rim_resolve::domain::EdgeWinner) -> Self {
        Self {
            after: value.after.as_str().to_string(),
            before: value.before.as_str().to_string(),
            layer: value.layer.into(),
            detail: value.detail.clone(),
        }
    }
}

/// Mirrors [`rim_resolve::domain::PatchFailureCause`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PatchFailureCauseDto {
    /// See [`rim_resolve::domain::PatchFailureCause::RemovedBy`].
    #[serde(rename_all = "camelCase")]
    RemovedBy {
        /// The earlier-loading mod whose own operation removes the node.
        mod_id: String,
    },
    /// See [`rim_resolve::domain::PatchFailureCause::NotYetInjected`].
    #[serde(rename_all = "camelCase")]
    NotYetInjected {
        /// The later-loading mod whose own operation injects the node.
        mod_id: String,
    },
    /// See [`rim_resolve::domain::PatchFailureCause::DeadTarget`].
    DeadTarget,
    /// See [`rim_resolve::domain::PatchFailureCause::Unknown`].
    Unknown,
}

impl From<&rim_resolve::domain::PatchFailureCause> for PatchFailureCauseDto {
    fn from(value: &rim_resolve::domain::PatchFailureCause) -> Self {
        match value {
            rim_resolve::domain::PatchFailureCause::RemovedBy(mod_id) => Self::RemovedBy {
                mod_id: mod_id.as_str().to_string(),
            },
            rim_resolve::domain::PatchFailureCause::NotYetInjected(mod_id) => {
                Self::NotYetInjected {
                    mod_id: mod_id.as_str().to_string(),
                }
            }
            rim_resolve::domain::PatchFailureCause::DeadTarget => Self::DeadTarget,
            rim_resolve::domain::PatchFailureCause::Unknown => Self::Unknown,
        }
    }
}

/// The full evidence behind a finding, for display. Mirrors [`Finding`]
/// one variant at a time; every set is a sorted list of mod ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FindingDto {
    /// See [`Finding::EdgeDropped`].
    #[serde(rename_all = "camelCase")]
    EdgeDropped {
        /// The edge's dependent side.
        after: String,
        /// The edge's dependency side.
        before: String,
        /// The kind of edge dropped.
        edge_kind: EdgeKindDto,
        /// The engine's own description.
        detail: String,
        /// The dropped edge's actual strength.
        strength: EdgeStrengthDto,
        /// The edge that overruled this one, when the witness cycle was a
        /// direct two-mod contradiction.
        winner: Option<EdgeWinnerDto>,
    },
    /// See [`Finding::DeclarationQuestioned`].
    #[serde(rename_all = "camelCase")]
    DeclarationQuestioned {
        /// The enforced edge's dependent side.
        declared_after: String,
        /// The enforced edge's dependency side.
        declared_before: String,
        /// Which layer the declared edge belongs to.
        declared_layer: LayerDto,
        /// A human-readable description of the declared edge.
        declared_detail: String,
        /// The advisory relation's kind.
        relation_kind: EdgeKindDto,
        /// A human-readable description of the advisory relation.
        relation_detail: String,
    },
    /// See [`Finding::DeclarationOverridden`].
    #[serde(rename_all = "camelCase")]
    DeclarationOverridden {
        /// The declared edge's dependent side.
        declared_after: String,
        /// The declared edge's dependency side.
        declared_before: String,
        /// The kind of declared edge overridden. Named `edge_kind`, not
        /// `kind` — this enum's own `#[serde(tag = "kind")]` discriminant
        /// already owns that JSON key, the same reason `EdgeDropped`
        /// above renames its identically-named field.
        edge_kind: EdgeKindDto,
        /// A human-readable description of the declared edge.
        detail: String,
        /// The declared-edge-override rule's edge that won.
        by: EdgeWinnerDto,
    },
    /// See [`Finding::AnyOfChoice`].
    AnyOfChoice {
        /// The mod requiring one of the candidates.
        after: String,
        /// The shared assembly name.
        assembly: String,
        /// Every candidate mod id.
        candidates: Vec<String>,
    },
    /// See [`Finding::DefOverride`].
    DefOverride {
        /// The contested def.
        key: DefKeyDto,
        /// Every owner, in load order.
        owners: Vec<String>,
        /// The owner whose def actually applies.
        winner: String,
    },
    /// See [`Finding::PatchCollision`].
    #[serde(rename_all = "camelCase")]
    PatchCollision {
        /// The patched def.
        key: DefKeyDto,
        /// Which attribute the patch predicate matched on.
        selector: SelectorDto,
        /// The path under the def the patches target, if any.
        sub_path: Option<String>,
        /// Every contributing mod, in load order.
        mods: Vec<String>,
    },
    /// See [`Finding::TextureOverride`].
    #[serde(rename_all = "camelCase")]
    TextureOverride {
        /// The shared, normalized texture path.
        texture_path: String,
        /// Every owner, in the order the scan saw them. Which one wins
        /// is `winner`, never the last of these.
        owners: Vec<String>,
        /// The owner loaded last in the selected order, whose file the
        /// game uses; computed in Rust, never re-derived from `owners`.
        winner: String,
    },
    /// See [`Finding::DuplicateAssembly`].
    #[serde(rename_all = "camelCase")]
    DuplicateAssembly {
        /// The shared assembly name.
        assembly_name: String,
        /// Every shipping mod, in load order.
        owners: Vec<String>,
    },
    /// See [`Finding::DuplicateTemplateName`].
    #[serde(rename_all = "camelCase")]
    DuplicateTemplateName {
        /// The shared template name.
        name: String,
        /// Every registering mod, in load order.
        owners: Vec<String>,
    },
    /// See [`Finding::KeyedTranslationCollision`].
    #[serde(rename_all = "camelCase")]
    KeyedTranslationCollision {
        /// One of the two mods.
        a: String,
        /// The other.
        b: String,
        /// Every key this pair collides over.
        keys: Vec<String>,
    },
    /// See [`Finding::SoundOverride`].
    #[serde(rename_all = "camelCase")]
    SoundOverride {
        /// The shared, normalized sound path.
        path: String,
        /// Every owner, in load order.
        owners: Vec<String>,
    },
    /// See [`Finding::UndeclaredTypeDependency`].
    #[serde(rename_all = "camelCase")]
    UndeclaredTypeDependency {
        /// The mod using the type.
        user: String,
        /// The mod whose DLL defines the type.
        provider: String,
        /// The type name.
        type_name: String,
    },
    /// See [`Finding::RuntimePatchCollision`].
    #[serde(rename_all = "camelCase")]
    RuntimePatchCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every patching mod, in load order.
        owners: Vec<String>,
    },
    /// See [`Finding::TranspilerCollision`].
    #[serde(rename_all = "camelCase")]
    TranspilerCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every mod declaring a `Transpiler` role on it, in load order.
        owners: Vec<String>,
    },
    /// See [`Finding::LikelyDuplicateMod`].
    #[serde(rename_all = "camelCase")]
    LikelyDuplicateMod {
        /// One of the two mods.
        a: String,
        /// The other.
        b: String,
        /// Identical def keys both mods define.
        shared_defs: usize,
    },
    /// See [`Finding::MissingMod`].
    #[serde(rename_all = "camelCase")]
    MissingMod {
        /// The missing mod.
        mod_id: String,
    },
    /// See [`Finding::MissingDependency`].
    #[serde(rename_all = "camelCase")]
    MissingDependency {
        /// The mod with the unmet dependency.
        mod_id: String,
        /// The missing dependency.
        dependency: String,
        /// The display name the author gave the dependency, if any.
        display_name: Option<String>,
    },
    /// See [`Finding::IncompatiblePair`].
    IncompatiblePair {
        /// One of the two mods.
        a: String,
        /// The other.
        b: String,
    },
    /// See [`Finding::UnsupportedVersion`].
    #[serde(rename_all = "camelCase")]
    UnsupportedVersion {
        /// The mod.
        mod_id: String,
    },
    /// See [`Finding::UndeclaredHardDependency`].
    UndeclaredHardDependency {
        /// The edge's dependent side.
        after: String,
        /// The edge's dependency side.
        before: String,
        /// The engine's own description.
        detail: String,
    },
    /// See [`Finding::LazyReferenceViolated`].
    LazyReferenceViolated {
        /// The edge's dependent side, base id.
        after: String,
        /// The edge's dependency side, base id.
        before: String,
        /// The engine's own description.
        detail: String,
    },
    /// See [`Finding::TagInferred`].
    #[serde(rename_all = "camelCase")]
    TagInferred {
        /// The tagged mod.
        mod_id: String,
        /// The inferred tag.
        tag: String,
        /// Every signal that matched.
        matched: Vec<TagSignalDto>,
    },
    /// See [`Finding::RuleOverruled`].
    #[serde(rename_all = "camelCase")]
    RuleOverruled {
        /// The losing rule's dependent side.
        after: String,
        /// The losing rule's dependency side.
        before: String,
        /// The losing rule's origin.
        origin: RuleOriginDto,
        /// The rule's own free-text note, if any.
        comment: Option<String>,
        /// The edge that overruled it, when the witness cycle was a
        /// direct two-mod contradiction.
        winner: Option<EdgeWinnerDto>,
        /// A path from `after` back around to `before`, populated only
        /// when `winner` is absent (a longer cycle).
        witness_cycle: Vec<String>,
    },
    /// See [`Finding::PlacementOverruled`].
    #[serde(rename_all = "camelCase")]
    PlacementOverruled {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it was pinned to.
        placement: PlacementDto,
        /// The placement rule's origin.
        origin: RuleOriginDto,
        /// The edge that overruled the placement.
        by: EdgeWinnerDto,
        /// Where the mod actually landed in the emitted order.
        landed_at: usize,
    },
    /// See [`Finding::PlacementQuestioned`].
    #[serde(rename_all = "camelCase")]
    PlacementQuestioned {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it's pinned to.
        placement: PlacementDto,
        /// The other mod named by the advisory relation.
        other: String,
        /// The advisory relation's kind.
        relation_kind: EdgeKindDto,
        /// A human-readable description of the advisory relation.
        relation_detail: String,
    },
    /// See [`Finding::PlacementOrderingOverridden`].
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
    /// See [`Finding::PlacementPromotesDependents`].
    #[serde(rename_all = "camelCase")]
    PlacementPromotesDependents {
        /// The pinned mod.
        mod_id: String,
        /// Which tier it's pinned to.
        placement: PlacementDto,
        /// Every other mod this placement promotes past its own tier
        /// boundary, sorted.
        promoted: Vec<String>,
    },
    /// See [`Finding::MissingTexturePath`].
    #[serde(rename_all = "camelCase")]
    MissingTexturePath {
        /// The mod whose own copy of `def` carries this field.
        referrer: String,
        /// The def carrying the field.
        def: DefKeyDto,
        /// The field's own tag name.
        field: String,
        /// The normalized path that resolved to no shipped file.
        path: String,
    },
    /// See [`Finding::PatchWillFail`].
    #[serde(rename_all = "camelCase")]
    PatchWillFail {
        /// The mod whose operation is predicted to fail.
        mod_id: String,
        /// The def or template the operation targets.
        def_key: DefKeyDto,
        /// Which attribute the operation's own target predicate matched
        /// on.
        selector: SelectorDto,
        /// The top-level operation's own RimWorld-log identity text —
        /// matches what a user sees in their own log.
        operation: String,
        /// The specific nested leaf's own xpath, when the failure traces
        /// to one identifiable leaf — diagnostic only.
        leaf_xpath: Option<String>,
        /// Why, classified from the replay.
        cause: PatchFailureCauseDto,
    },
    /// See [`Finding::ContributesNothing`].
    #[serde(rename_all = "camelCase")]
    ContributesNothing {
        /// The mod whose every contribution is inert.
        mod_id: String,
    },
    /// See [`Finding::UndecodableTexture`].
    #[serde(rename_all = "camelCase")]
    UndecodableTexture {
        /// The mod shipping the file.
        mod_id: String,
        /// The normalized key.
        path: String,
        /// The DDS header's own width, in pixels.
        width: u32,
        /// The DDS header's own height, in pixels.
        height: u32,
        /// The pixel format's four-character-code tag; empty when the
        /// header was too malformed to read one.
        fourcc: String,
        /// Whether this mod also ships a non-`.dds` file at the same key
        /// (never actually loaded instead — evidence only).
        has_png_sibling: bool,
    },
    /// See [`Finding::BrokenInheritance`].
    #[serde(rename_all = "camelCase")]
    BrokenInheritance {
        /// The mod whose def/template references `parent_name`.
        mod_id: String,
        /// The unresolved or wrong-typed `ParentName` value.
        parent_name: String,
        /// The representative referencing def/template.
        child: DefRefPartsDto,
        /// Which problem, and (for a type mismatch) what it resolved to.
        problem: InheritanceProblemDto,
        /// Every active, gate-open concrete def that loses inherited
        /// content, bounded.
        affected: Vec<DefKeyDto>,
        /// How many more affected defs existed beyond `affected`'s own
        /// cap. `0` when nothing was truncated.
        truncated: usize,
    },
    /// See [`Finding::NearMissModReference`].
    #[serde(rename_all = "camelCase")]
    NearMissModReference {
        /// The mod whose own file carries the written reference.
        referrer: String,
        /// Whether `written` is a `FindMod` name or a `MayRequire` id.
        /// Named `referenceKind`, not `kind` — this enum's own
        /// `#[serde(tag = "kind")]` discriminant already claims that
        /// field name at the wire level, and a struct variant's own
        /// field of the same name would collide with it.
        reference_kind: ModReferenceKindDto,
        /// The value as written.
        written: String,
        /// The closest active mod.
        candidate: String,
        /// `candidate`'s own display name, for evidence.
        candidate_name: String,
        /// Which similarity rule flagged this pair.
        rule: NearMissRuleDto,
        /// The file the reference was read from, when known.
        file: Option<String>,
    },
    /// See [`Finding::DiscardedAddition`].
    #[serde(rename_all = "camelCase")]
    DiscardedAddition {
        /// The mod whose `PatchOperationReplace` discards `adder`'s own
        /// content.
        replacer: String,
        /// The mod whose earlier addition is discarded.
        adder: String,
        /// The replaced def.
        def: DefKeyDto,
        /// The replaced node's own display path.
        path: String,
        /// `adder`'s own op's target display path.
        adder_path: String,
    },
    /// See [`Finding::DanglingDefReference`].
    #[serde(rename_all = "camelCase")]
    DanglingDefReference {
        /// The dangling `defName`.
        name: String,
        /// Every distinct referrer, bounded.
        referrers: Vec<RefSiteSummaryDto>,
        /// How many more referrers existed beyond `referrers`'s own cap.
        /// `0` when nothing was truncated.
        truncated_referrers: usize,
        /// Why the name never resolves.
        cause: DanglingCauseDto,
        /// Set when the referencing field's own resolved values are
        /// mostly `SoundDef`s — a missing `SoundDef` falls back to an
        /// undefined sound rather than failing to load.
        likely_sound: bool,
    },
}

/// Mirrors [`rim_analyzer::domain::DefRefParts`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefRefPartsDto {
    /// The referencing node's own tag (`ThingDef`, `HediffDef`, ...).
    pub def_type: String,
    /// The referencing node's `defName`, or its `Name` attribute when
    /// `is_template` is set.
    pub name: String,
    /// Whether `name` is a `Name` attribute (an abstract template)
    /// rather than a `defName`.
    pub is_template: bool,
}

impl From<&rim_analyzer::domain::DefRefParts> for DefRefPartsDto {
    fn from(value: &rim_analyzer::domain::DefRefParts) -> Self {
        Self {
            def_type: value.def_type.clone(),
            name: value.name.clone(),
            is_template: value.is_template,
        }
    }
}

/// Mirrors [`rim_analyzer::domain::InheritanceProblem`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum InheritanceProblemDto {
    /// See [`rim_analyzer::domain::InheritanceProblem::MissingParent`].
    MissingParent,
    /// See [`rim_analyzer::domain::InheritanceProblem::ParentTypeMismatch`].
    #[serde(rename_all = "camelCase")]
    ParentTypeMismatch {
        /// The resolved parent registration's own element tag.
        parent_type: String,
        /// The mod owning that registration, when it has one specific
        /// owner (vanilla and patch-injected registrations don't).
        parent_owner: Option<String>,
    },
}

impl From<&rim_analyzer::domain::InheritanceProblem> for InheritanceProblemDto {
    fn from(value: &rim_analyzer::domain::InheritanceProblem) -> Self {
        match value {
            rim_analyzer::domain::InheritanceProblem::MissingParent => Self::MissingParent,
            rim_analyzer::domain::InheritanceProblem::ParentTypeMismatch {
                parent_type,
                parent_owner,
            } => Self::ParentTypeMismatch {
                parent_type: parent_type.clone(),
                parent_owner: parent_owner.as_ref().map(|id| id.as_str().to_string()),
            },
        }
    }
}

/// Mirrors [`rim_analyzer::domain::RefSiteSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RefSiteSummaryDto {
    /// Where the reference was written.
    pub referrer: RefSiteReferrerDto,
    /// Field path from the referrer's own root, `li` segments collapsed.
    pub field_path: String,
}

impl From<&rim_analyzer::domain::RefSiteSummary> for RefSiteSummaryDto {
    fn from(value: &rim_analyzer::domain::RefSiteSummary) -> Self {
        Self {
            referrer: (&value.referrer).into(),
            field_path: value.field_path.clone(),
        }
    }
}

/// Mirrors [`rim_analyzer::domain::RefSiteReferrer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RefSiteReferrerDto {
    /// See [`rim_analyzer::domain::RefSiteReferrer::ReferencedFromDef`].
    #[serde(rename_all = "camelCase")]
    ReferencedFromDef {
        /// The mod owning the referencing def.
        mod_id: String,
        /// The referencing def's own element tag.
        def_type: String,
        /// The referencing def's own `defName`.
        def_name: String,
    },
    /// See [`rim_analyzer::domain::RefSiteReferrer::ReferencedFromTemplate`].
    #[serde(rename_all = "camelCase")]
    ReferencedFromTemplate {
        /// The mod owning the referencing template.
        mod_id: String,
        /// The referencing template's own element tag.
        def_type: String,
        /// The referencing template's own `Name` attribute.
        name: String,
    },
    /// See [`rim_analyzer::domain::RefSiteReferrer::ReferencedFromPatch`].
    #[serde(rename_all = "camelCase")]
    ReferencedFromPatch {
        /// The mod owning the referencing patch op.
        mod_id: String,
        /// The def type the patch op targets.
        def_type: String,
        /// The def name the patch op targets.
        def_name: String,
        /// The patch file the reference was read from, when known.
        file: Option<String>,
    },
}

impl From<&rim_analyzer::domain::RefSiteReferrer> for RefSiteReferrerDto {
    fn from(value: &rim_analyzer::domain::RefSiteReferrer) -> Self {
        match value {
            rim_analyzer::domain::RefSiteReferrer::ReferencedFromDef {
                mod_id,
                def_type,
                def_name,
            } => Self::ReferencedFromDef {
                mod_id: mod_id.as_str().to_string(),
                def_type: def_type.clone(),
                def_name: def_name.clone(),
            },
            rim_analyzer::domain::RefSiteReferrer::ReferencedFromTemplate {
                mod_id,
                def_type,
                name,
            } => Self::ReferencedFromTemplate {
                mod_id: mod_id.as_str().to_string(),
                def_type: def_type.clone(),
                name: name.clone(),
            },
            rim_analyzer::domain::RefSiteReferrer::ReferencedFromPatch {
                mod_id,
                def_type,
                def_name,
                locator,
            } => Self::ReferencedFromPatch {
                mod_id: mod_id.as_str().to_string(),
                def_type: def_type.clone(),
                def_name: def_name.clone(),
                file: Some(locator.file.display().to_string()),
            },
        }
    }
}

/// Mirrors [`rim_analyzer::domain::DanglingCause`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DanglingCauseDto {
    /// See [`rim_analyzer::domain::DanglingCause::RemovedBy`].
    #[serde(rename_all = "camelCase")]
    RemovedBy {
        /// The mod whose patch removed it.
        mod_id: String,
        /// The removing patch's own file, when known.
        file: Option<String>,
    },
    /// See [`rim_analyzer::domain::DanglingCause::OnlyInUnloadedFolder`].
    #[serde(rename_all = "camelCase")]
    OnlyInUnloadedFolder {
        /// The mod whose own unloaded folder defines it.
        mod_id: String,
        /// The unloaded folder's own name.
        folder: String,
    },
    /// See [`rim_analyzer::domain::DanglingCause::OnlyInInactiveMod`].
    #[serde(rename_all = "camelCase")]
    OnlyInInactiveMod {
        /// The inactive mod that defines it.
        mod_id: String,
    },
    /// See [`rim_analyzer::domain::DanglingCause::DefinedNowhere`].
    DefinedNowhere,
    /// See [`rim_analyzer::domain::DanglingCause::Unexplained`].
    Unexplained,
}

impl From<&rim_analyzer::domain::DanglingCause> for DanglingCauseDto {
    fn from(value: &rim_analyzer::domain::DanglingCause) -> Self {
        match value {
            rim_analyzer::domain::DanglingCause::RemovedBy { mod_id, locator } => Self::RemovedBy {
                mod_id: mod_id.as_str().to_string(),
                file: Some(locator.file.display().to_string()),
            },
            rim_analyzer::domain::DanglingCause::OnlyInUnloadedFolder { mod_id, folder } => {
                Self::OnlyInUnloadedFolder {
                    mod_id: mod_id.as_str().to_string(),
                    folder: folder.clone(),
                }
            }
            rim_analyzer::domain::DanglingCause::OnlyInInactiveMod { mod_id } => {
                Self::OnlyInInactiveMod {
                    mod_id: mod_id.as_str().to_string(),
                }
            }
            rim_analyzer::domain::DanglingCause::DefinedNowhere => Self::DefinedNowhere,
            rim_analyzer::domain::DanglingCause::Unexplained => Self::Unexplained,
        }
    }
}

/// Mirrors [`rim_analyzer::domain::ModReferenceKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModReferenceKindDto {
    /// See [`rim_analyzer::domain::ModReferenceKind::FindModName`].
    FindModName,
    /// See [`rim_analyzer::domain::ModReferenceKind::MayRequireId`].
    MayRequireId,
}

impl From<rim_analyzer::domain::ModReferenceKind> for ModReferenceKindDto {
    fn from(value: rim_analyzer::domain::ModReferenceKind) -> Self {
        match value {
            rim_analyzer::domain::ModReferenceKind::FindModName => Self::FindModName,
            rim_analyzer::domain::ModReferenceKind::MayRequireId => Self::MayRequireId,
        }
    }
}

/// Mirrors [`rim_analyzer::domain::NearMissRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum NearMissRuleDto {
    /// See [`rim_analyzer::domain::NearMissRule::CaseOnly`].
    CaseOnly,
    /// See [`rim_analyzer::domain::NearMissRule::Normalized`].
    Normalized,
    /// See [`rim_analyzer::domain::NearMissRule::NearMiss`].
    NearMiss,
    /// See [`rim_analyzer::domain::NearMissRule::LeadingToken`].
    LeadingToken,
}

impl From<rim_analyzer::domain::NearMissRule> for NearMissRuleDto {
    fn from(value: rim_analyzer::domain::NearMissRule) -> Self {
        match value {
            rim_analyzer::domain::NearMissRule::CaseOnly => Self::CaseOnly,
            rim_analyzer::domain::NearMissRule::Normalized => Self::Normalized,
            rim_analyzer::domain::NearMissRule::NearMiss => Self::NearMiss,
            rim_analyzer::domain::NearMissRule::LeadingToken => Self::LeadingToken,
        }
    }
}

impl From<&Finding> for FindingDto {
    fn from(value: &Finding) -> Self {
        match value {
            Finding::EdgeDropped {
                after,
                before,
                kind,
                detail,
                strength,
                winner,
            } => Self::EdgeDropped {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                edge_kind: (*kind).into(),
                detail: detail.clone(),
                strength: (*strength).into(),
                winner: winner.as_ref().map(Into::into),
            },
            Finding::DeclarationQuestioned {
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
            Finding::DeclarationOverridden {
                declared_after,
                declared_before,
                kind,
                detail,
                by,
            } => Self::DeclarationOverridden {
                declared_after: declared_after.as_str().to_string(),
                declared_before: declared_before.as_str().to_string(),
                edge_kind: (*kind).into(),
                detail: detail.clone(),
                by: by.into(),
            },
            Finding::AnyOfChoice {
                after,
                assembly,
                candidates,
            } => Self::AnyOfChoice {
                after: after.as_str().to_string(),
                assembly: assembly.clone(),
                candidates: candidates
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
            },
            Finding::DefOverride {
                key,
                owners,
                winner,
            } => Self::DefOverride {
                key: key.clone().into(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
                winner: winner.as_str().to_string(),
            },
            Finding::PatchCollision {
                key,
                selector,
                sub_path,
                mods,
                ..
            } => Self::PatchCollision {
                key: key.clone().into(),
                selector: (*selector).into(),
                sub_path: sub_path.clone(),
                mods: mods.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::TextureOverride {
                texture_path,
                owners,
                winner,
            } => Self::TextureOverride {
                texture_path: texture_path.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
                winner: winner.as_str().to_string(),
            },
            Finding::DuplicateAssembly {
                assembly_name,
                owners,
            } => Self::DuplicateAssembly {
                assembly_name: assembly_name.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::DuplicateTemplateName { name, owners } => Self::DuplicateTemplateName {
                name: name.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::KeyedTranslationCollision { a, b, keys } => Self::KeyedTranslationCollision {
                a: a.as_str().to_string(),
                b: b.as_str().to_string(),
                keys: keys.clone(),
            },
            Finding::SoundOverride { path, owners } => Self::SoundOverride {
                path: path.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::UndeclaredTypeDependency {
                user,
                provider,
                type_name,
            } => Self::UndeclaredTypeDependency {
                user: user.as_str().to_string(),
                provider: provider.as_str().to_string(),
                type_name: type_name.clone(),
            },
            Finding::RuntimePatchCollision {
                target_type,
                target_method,
                owners,
            } => Self::RuntimePatchCollision {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::TranspilerCollision {
                target_type,
                target_method,
                owners,
            } => Self::TranspilerCollision {
                target_type: target_type.clone(),
                target_method: target_method.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::LikelyDuplicateMod { a, b, shared_defs } => Self::LikelyDuplicateMod {
                a: a.as_str().to_string(),
                b: b.as_str().to_string(),
                shared_defs: *shared_defs,
            },
            Finding::MissingMod { mod_id } => Self::MissingMod {
                mod_id: mod_id.as_str().to_string(),
            },
            Finding::MissingDependency {
                mod_id,
                dependency,
                display_name,
            } => Self::MissingDependency {
                mod_id: mod_id.as_str().to_string(),
                dependency: dependency.as_str().to_string(),
                display_name: display_name.clone(),
            },
            Finding::IncompatiblePair { a, b } => Self::IncompatiblePair {
                a: a.as_str().to_string(),
                b: b.as_str().to_string(),
            },
            Finding::UnsupportedVersion { mod_id } => Self::UnsupportedVersion {
                mod_id: mod_id.as_str().to_string(),
            },
            Finding::UndeclaredHardDependency {
                after,
                before,
                detail,
            } => Self::UndeclaredHardDependency {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                detail: detail.clone(),
            },
            Finding::LazyReferenceViolated {
                after,
                before,
                detail,
            } => Self::LazyReferenceViolated {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                detail: detail.clone(),
            },
            Finding::TagInferred {
                mod_id,
                tag,
                matched,
            } => Self::TagInferred {
                mod_id: mod_id.as_str().to_string(),
                tag: tag.as_str().to_string(),
                matched: matched.iter().map(TagSignalDto::from).collect(),
            },
            Finding::RuleOverruled {
                after,
                before,
                origin,
                comment,
                winner,
                witness_cycle,
                // Not surfaced on this DTO: it only ever feeds the
                // server-side confidence table and rationale wording
                // (`ledger::suggest::rule_overruled`), both already
                // reflected in `Resolution`/`Suggestion` — a raw boolean
                // duplicating that would need a UI decision of its own
                // this task didn't ask for.
                overrides_declared: _,
            } => Self::RuleOverruled {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                origin: (*origin).into(),
                comment: comment.clone(),
                winner: winner.as_ref().map(EdgeWinnerDto::from),
                witness_cycle: witness_cycle
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
            },
            Finding::PlacementOverruled {
                mod_id,
                placement,
                origin,
                by,
                landed_at,
            } => Self::PlacementOverruled {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                origin: (*origin).into(),
                by: by.into(),
                landed_at: *landed_at,
            },
            Finding::PlacementQuestioned {
                mod_id,
                placement,
                other,
                relation_kind,
                relation_detail,
            } => Self::PlacementQuestioned {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                other: other.as_str().to_string(),
                relation_kind: (*relation_kind).into(),
                relation_detail: relation_detail.clone(),
            },
            Finding::PlacementOrderingOverridden {
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
            Finding::PlacementPromotesDependents {
                mod_id,
                placement,
                promoted,
            } => Self::PlacementPromotesDependents {
                mod_id: mod_id.as_str().to_string(),
                placement: (*placement).into(),
                promoted: promoted.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Finding::MissingTexturePath {
                referrer,
                def,
                field,
                path,
            } => Self::MissingTexturePath {
                referrer: referrer.as_str().to_string(),
                def: def.clone().into(),
                field: field.clone(),
                path: path.clone(),
            },
            Finding::PatchWillFail {
                mod_id,
                def_key,
                selector,
                operation,
                leaf_xpath,
                cause,
                // `Finding::PatchWillFail` never reaches the ordinary
                // ledger/inbox path this generic conversion serves (see
                // `rim-session`'s own doc comment on `VerifyOrder`) — this
                // arm exists only so the match stays exhaustive. The
                // richer `reorderKind`/`reorder` shape lives on
                // `dto::verify::VerifyDefTargetDto` instead, which is
                // what the apply dialog's verify view actually reads.
                reorder_kind: _,
            } => Self::PatchWillFail {
                mod_id: mod_id.as_str().to_string(),
                def_key: def_key.clone().into(),
                selector: (*selector).into(),
                operation: operation.clone(),
                leaf_xpath: leaf_xpath.clone(),
                cause: cause.into(),
            },
            Finding::ContributesNothing { mod_id } => Self::ContributesNothing {
                mod_id: mod_id.as_str().to_string(),
            },
            Finding::UndecodableTexture {
                mod_id,
                path,
                width,
                height,
                fourcc,
                has_png_sibling,
            } => Self::UndecodableTexture {
                mod_id: mod_id.as_str().to_string(),
                path: path.clone(),
                width: *width,
                height: *height,
                fourcc: fourcc.clone(),
                has_png_sibling: *has_png_sibling,
            },
            Finding::BrokenInheritance {
                mod_id,
                parent_name,
                child,
                problem,
                affected,
                truncated,
            } => Self::BrokenInheritance {
                mod_id: mod_id.as_str().to_string(),
                parent_name: parent_name.clone(),
                child: child.into(),
                problem: problem.into(),
                affected: affected
                    .iter()
                    .map(|(def_type, def_name)| DefKeyDto {
                        def_type: def_type.clone(),
                        def_name: def_name.clone(),
                    })
                    .collect(),
                truncated: *truncated,
            },
            Finding::NearMissModReference {
                referrer,
                kind,
                written,
                candidate,
                candidate_name,
                rule,
                locator,
            } => Self::NearMissModReference {
                referrer: referrer.as_str().to_string(),
                reference_kind: (*kind).into(),
                written: written.clone(),
                candidate: candidate.as_str().to_string(),
                candidate_name: candidate_name.clone(),
                rule: (*rule).into(),
                file: locator
                    .as_ref()
                    .map(|locator| locator.file.display().to_string()),
            },
            Finding::DiscardedAddition {
                replacer,
                adder,
                def,
                path,
                adder_path,
            } => Self::DiscardedAddition {
                replacer: replacer.as_str().to_string(),
                adder: adder.as_str().to_string(),
                def: def.clone().into(),
                path: path.clone(),
                adder_path: adder_path.clone(),
            },
            Finding::DanglingDefReference {
                name,
                referrers,
                truncated_referrers,
                cause,
                likely_sound,
            } => Self::DanglingDefReference {
                name: name.clone(),
                referrers: referrers.iter().map(Into::into).collect(),
                truncated_referrers: *truncated_referrers,
                cause: cause.into(),
                likely_sound: *likely_sound,
            },
        }
    }
}
