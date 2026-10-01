//! DTOs for `list_mod_changes`/`inspect_def`/`search_defs`: the def
//! inspector's commands.
//!
//! Filtering, paging, and aggregation over one mod's inventory live in
//! `rim-session` (`Session::changes`/`Session::search_defs`, and
//! `rim_session::effective_fields::page` for
//! [`DefInspectionDto::fields`]), per `apps/desktop/CLAUDE.md`'s usual
//! rule; this module only maps that output to DTOs, through
//! [`EffectiveFieldFilterDto`]'s `From` conversion — the same shape
//! [`ChangeFilterDto`]'s own `From` already uses.

use std::collections::BTreeMap;

use rim_analyzer::domain::{FindModGate, ModId, Selector};
use rim_merge::effective::{Completeness, Provenance, Stopper};
use rim_resolve::domain::{DefKey, DefRef};
use rim_session::effective_fields::{EffectiveField, EffectiveFieldFilter};
use rim_session::use_cases::{DefInspection, PatchOpSummary, Patcher, TemplateAmbiguity, Toucher};
use rim_session::{AssetKind, ChangeFilter, ChangeKind, ChangePage, ChangeRow};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{DefKeyDto, OrderSourceDto, ResolutionStatusDto};
use super::merge::CaveatDto;

/// Mirrors [`ChangeKind`], flattened (like [`super::finding::FindingKindDto`]
/// mirrors `FindingKind`): [`ChangeKind::OverridesAsset`]'s payload
/// becomes three flat variants instead of a nested `assetKind` field,
/// since every call site (the filter request, the kind-chip counts) wants
/// one closed, six-way enum to switch on, not a tagged union one level
/// deeper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ChangeKindDto {
    /// See [`ChangeKind::OwnsDef`].
    OwnsDef,
    /// See [`ChangeKind::OwnsTemplate`].
    OwnsTemplate,
    /// See [`ChangeKind::PatchesDef`].
    PatchesDef,
    /// See [`ChangeKind::OverridesAsset`] with [`AssetKind::Texture`].
    OverridesTexture,
    /// See [`ChangeKind::OverridesAsset`] with [`AssetKind::Sound`].
    OverridesSound,
    /// See [`ChangeKind::OverridesAsset`] with [`AssetKind::KeyedTranslation`].
    OverridesKeyedTranslation,
}

impl From<ChangeKind> for ChangeKindDto {
    fn from(value: ChangeKind) -> Self {
        match value {
            ChangeKind::OwnsDef => Self::OwnsDef,
            ChangeKind::OwnsTemplate => Self::OwnsTemplate,
            ChangeKind::PatchesDef => Self::PatchesDef,
            ChangeKind::OverridesAsset(AssetKind::Texture) => Self::OverridesTexture,
            ChangeKind::OverridesAsset(AssetKind::Sound) => Self::OverridesSound,
            ChangeKind::OverridesAsset(AssetKind::KeyedTranslation) => {
                Self::OverridesKeyedTranslation
            }
        }
    }
}

impl From<ChangeKindDto> for ChangeKind {
    fn from(value: ChangeKindDto) -> Self {
        match value {
            ChangeKindDto::OwnsDef => Self::OwnsDef,
            ChangeKindDto::OwnsTemplate => Self::OwnsTemplate,
            ChangeKindDto::PatchesDef => Self::PatchesDef,
            ChangeKindDto::OverridesTexture => Self::OverridesAsset(AssetKind::Texture),
            ChangeKindDto::OverridesSound => Self::OverridesAsset(AssetKind::Sound),
            ChangeKindDto::OverridesKeyedTranslation => {
                Self::OverridesAsset(AssetKind::KeyedTranslation)
            }
        }
    }
}

/// Request shape for `list_mod_changes`. Mirrors [`ChangeFilter`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ChangeFilterDto {
    /// Only rows of one of these kinds.
    pub kinds: Option<Vec<ChangeKindDto>>,
    /// Only rows whose def/template name, type, or asset path contains
    /// this substring (case-insensitive).
    pub search: Option<String>,
    /// How many matching rows to skip.
    pub offset: usize,
    /// How many rows to return, capped at `rim_session::MAX_PAGE_SIZE`.
    pub limit: usize,
}

impl From<&ChangeFilterDto> for ChangeFilter {
    fn from(value: &ChangeFilterDto) -> Self {
        Self {
            kinds: value
                .kinds
                .as_ref()
                .map(|kinds| kinds.iter().copied().map(Into::into).collect()),
            search: value.search.clone(),
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// One row of `list_mod_changes`. Mirrors [`ChangeRow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ChangeRowDto {
    /// What kind of change this is.
    pub kind: ChangeKindDto,
    /// The def/template's canonical ref text — links to the def page.
    /// `null` for [`ChangeKindDto::OverridesTexture`]/`OverridesSound`/
    /// `OverridesKeyedTranslation` (an asset path, not a def).
    pub def_ref: Option<String>,
    /// The shared asset path/key. `null` for every def-shaped kind.
    pub asset_path: Option<String>,
    /// How many other active mods also touch this target.
    pub other_touchers: usize,
    /// This mod's own top-level patch-op count on the target.
    pub op_count: usize,
    /// Every finding this mod is a genuine party to on this target,
    /// canonical key text.
    pub finding_keys: Vec<String>,
}

impl From<&ChangeRow> for ChangeRowDto {
    fn from(value: &ChangeRow) -> Self {
        Self {
            kind: value.kind.into(),
            def_ref: value.def_ref.as_ref().map(ToString::to_string),
            asset_path: value.asset_path.clone(),
            other_touchers: value.other_touchers,
            op_count: value.op_count,
            finding_keys: value.finding_keys.iter().map(ToString::to_string).collect(),
        }
    }
}

/// How many rows fall under each [`ChangeKindDto`]. A fixed-field struct
/// rather than a `BTreeMap<ChangeKindDto, usize>`, matching
/// [`super::finding::FindingKindCountsDto`]'s own precedent: the bucket
/// set is small, closed, and stable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ChangeKindCountsDto {
    /// See [`ChangeKindDto::OwnsDef`].
    pub owns_def: usize,
    /// See [`ChangeKindDto::OwnsTemplate`].
    pub owns_template: usize,
    /// See [`ChangeKindDto::PatchesDef`].
    pub patches_def: usize,
    /// See [`ChangeKindDto::OverridesTexture`].
    pub overrides_texture: usize,
    /// See [`ChangeKindDto::OverridesSound`].
    pub overrides_sound: usize,
    /// See [`ChangeKindDto::OverridesKeyedTranslation`].
    pub overrides_keyed_translation: usize,
}

impl From<&BTreeMap<ChangeKind, usize>> for ChangeKindCountsDto {
    fn from(value: &BTreeMap<ChangeKind, usize>) -> Self {
        let mut counts = Self::default();
        for (kind, count) in value {
            match kind {
                ChangeKind::OwnsDef => counts.owns_def = *count,
                ChangeKind::OwnsTemplate => counts.owns_template = *count,
                ChangeKind::PatchesDef => counts.patches_def = *count,
                ChangeKind::OverridesAsset(AssetKind::Texture) => counts.overrides_texture = *count,
                ChangeKind::OverridesAsset(AssetKind::Sound) => counts.overrides_sound = *count,
                ChangeKind::OverridesAsset(AssetKind::KeyedTranslation) => {
                    counts.overrides_keyed_translation = *count;
                }
            }
        }
        counts
    }
}

/// One page of `list_mod_changes`. Mirrors [`ChangePage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ChangePageDto {
    /// Total rows matching the filter, before paging.
    pub total: usize,
    /// This page's rows.
    pub items: Vec<ChangeRowDto>,
    /// Counts per kind over the whole matching set (search-filtered, not
    /// kind-filtered — see [`ChangePage::kind_counts`]'s own doc comment).
    pub kind_counts: ChangeKindCountsDto,
}

impl From<&ChangePage> for ChangePageDto {
    fn from(value: &ChangePage) -> Self {
        Self {
            total: value.total,
            items: value.items.iter().map(Into::into).collect(),
            kind_counts: (&value.kind_counts).into(),
        }
    }
}

/// One hit of `search_defs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefSearchHitDto {
    /// The matched def/template's canonical ref text.
    pub def_ref: String,
    /// How many active mods own it.
    pub owners: usize,
}

// -- `inspect_def` -----------------------------------------------------

/// Request shape for `inspect_def`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct InspectDefRequestDto {
    /// The def/template's canonical ref text (see [`DefRef`]'s `Display`).
    pub def_ref: String,
    /// Filters and pages [`DefInspectionDto::fields`].
    pub filter: EffectiveFieldFilterDto,
}

/// Filters and pages a [`DefInspectionDto`]'s effective-field list — the
/// same `offset`/`limit` accumulation shape as
/// [`super::merge::MergeFieldFilterDto`]. Mirrors [`EffectiveFieldFilter`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EffectiveFieldFilterDto {
    /// Hide [`ProvenanceDto::Owner`] rows — a field the winner's own raw
    /// node set, untouched by any patch or inheritance.
    pub only_patched: bool,
    /// How many matching rows to skip.
    pub offset: usize,
    /// How many rows to return, capped at `rim_session::MAX_PAGE_SIZE`.
    pub limit: usize,
}

impl From<&EffectiveFieldFilterDto> for EffectiveFieldFilter {
    fn from(value: &EffectiveFieldFilterDto) -> Self {
        Self {
            only_patched: value.only_patched,
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// One owner/registrant in a [`DefInspectionDto`]. Mirrors [`Toucher`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefToucherDto {
    /// The owning/registering mod.
    pub mod_id: String,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether it's a Rimmerge-generated mod.
    pub is_generated: bool,
}

impl From<&Toucher> for DefToucherDto {
    fn from(value: &Toucher) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            position: value.position,
            is_generated: value.is_generated,
        }
    }
}

/// Mirrors [`FindModGate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FindModGateDto {
    /// See [`FindModGate::AnyActive`].
    AnyActive {
        /// The mod display names, any of which opens this gate.
        mods: Vec<String>,
    },
    /// See [`FindModGate::NoneActive`].
    NoneActive {
        /// The mod display names, none of which must be active.
        mods: Vec<String>,
    },
}

impl From<&FindModGate> for FindModGateDto {
    fn from(value: &FindModGate) -> Self {
        match value {
            FindModGate::AnyActive(mods) => Self::AnyActive { mods: mods.clone() },
            FindModGate::NoneActive(mods) => Self::NoneActive { mods: mods.clone() },
        }
    }
}

/// One top-level patch operation's own identity. Mirrors [`PatchOpSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchOpSummaryDto {
    /// The `Class` attribute, e.g. `PatchOperationReplace`.
    pub class: String,
    /// The raw `<xpath>` text, verbatim.
    pub xpath: Option<String>,
    /// Whatever followed the `defName="..."]`/`Name="..."]` bracket.
    pub sub_path: Option<String>,
    /// One gate per enclosing `PatchOperationFindMod`.
    pub find_mod_context: Vec<FindModGateDto>,
    /// `MayRequire` packageIds.
    pub may_require: Vec<String>,
    /// `MayRequireAnyOf` packageIds.
    pub may_require_any_of: Vec<String>,
    /// Where this operation lives on disk, for display.
    pub locator_file: String,
    /// Whether this summary describes a stand-in descendant rather than
    /// the top-level `<Operation>` node itself — see
    /// [`PatchOpSummary::is_wrapped`].
    pub is_wrapped: bool,
}

impl From<&PatchOpSummary> for PatchOpSummaryDto {
    fn from(value: &PatchOpSummary) -> Self {
        Self {
            class: value.class.clone(),
            xpath: value.xpath.clone(),
            sub_path: value.sub_path.clone(),
            find_mod_context: value.find_mod_context.iter().map(Into::into).collect(),
            may_require: value.may_require.clone(),
            may_require_any_of: value.may_require_any_of.clone(),
            locator_file: value.locator_file.display().to_string(),
            is_wrapped: value.is_wrapped,
        }
    }
}

/// One mod patching the inspected def/template. Mirrors [`Patcher`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatcherDto {
    /// The patching mod.
    pub mod_id: String,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether it's a Rimmerge-generated mod.
    pub is_generated: bool,
    /// One summary per top-level operation this mod aims at the target.
    pub ops: Vec<PatchOpSummaryDto>,
    /// `null` when this mod's own ops replayed cleanly as part of the
    /// real, full load-ordered fold (or the fold never reached them);
    /// otherwise the fold's own stopper error, verbatim — see
    /// [`Patcher::replay`].
    pub replay_error: Option<String>,
    /// Whether the fold actually got as far as this mod's own ops before
    /// stopping (if it stopped at all) — `false` means `replayError`
    /// being `null` is "not reached", not a verified success. See
    /// [`Patcher::reached`].
    pub reached: bool,
    /// Non-blocking issues the fold recorded for this mod's own
    /// contributions (e.g. an op that matched nothing). See
    /// [`Patcher::caveats`].
    pub caveats: Vec<CaveatDto>,
}

impl From<&Patcher> for PatcherDto {
    fn from(value: &Patcher) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            position: value.position,
            is_generated: value.is_generated,
            ops: value.ops.iter().map(Into::into).collect(),
            replay_error: value.replay.as_ref().err().cloned(),
            reached: value.reached,
            caveats: value.caveats.iter().map(CaveatDto::from).collect(),
        }
    }
}

/// Which stage last set one effective field. Mirrors [`Provenance`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ProvenanceDto {
    /// See [`Provenance::Owner`].
    #[serde(rename_all = "camelCase")]
    Owner {
        /// The winning owner.
        mod_id: String,
    },
    /// See [`Provenance::Patch`].
    #[serde(rename_all = "camelCase")]
    Patch {
        /// The mod that shipped the operation.
        mod_id: String,
        /// The operation's index among every contribution on this def.
        op_index: usize,
    },
    /// See [`Provenance::Inherited`].
    Inherited {
        /// The supplying template.
        template: DefKeyDto,
        /// The mod that registered that template.
        owner: String,
    },
    /// See [`Provenance::UnattributedTemplate`].
    UnattributedTemplate {
        /// The supplying template, whose owner isn't known.
        template: DefKeyDto,
    },
}

impl From<&Provenance> for ProvenanceDto {
    fn from(value: &Provenance) -> Self {
        match value {
            Provenance::Owner(mod_id) => Self::Owner {
                mod_id: mod_id.as_str().to_string(),
            },
            Provenance::Patch { mod_id, op_index } => Self::Patch {
                mod_id: mod_id.as_str().to_string(),
                op_index: *op_index,
            },
            Provenance::Inherited { template, owner } => Self::Inherited {
                template: template.clone().into(),
                owner: owner.as_str().to_string(),
            },
            Provenance::UnattributedTemplate { template } => Self::UnattributedTemplate {
                template: template.clone().into(),
            },
        }
    }
}

/// Why the effective-def pipeline stopped short of `Complete`. Mirrors
/// [`Stopper`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum StopperDto {
    /// See [`Stopper::Replay`].
    #[serde(rename_all = "camelCase")]
    Replay {
        /// The mod that shipped the operation.
        mod_id: String,
        /// The operation's index among every contribution on this def.
        op_index: usize,
        /// Why the replay stopped, verbatim.
        error: String,
    },
    /// See [`Stopper::Inherit`].
    Inherit {
        /// Why the `ParentName` chain couldn't be resolved, verbatim.
        reason: String,
    },
}

impl From<&Stopper> for StopperDto {
    fn from(value: &Stopper) -> Self {
        match value {
            Stopper::Replay {
                mod_id,
                op_index,
                error,
            } => Self::Replay {
                mod_id: mod_id.as_str().to_string(),
                op_index: *op_index,
                error: error.to_string(),
            },
            Stopper::Inherit(error) => Self::Inherit {
                reason: error.to_string(),
            },
        }
    }
}

/// Whether the effective-def pipeline made it through every stage.
/// Mirrors [`Completeness`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CompletenessDto {
    /// See [`Completeness::Complete`].
    Complete,
    /// See [`Completeness::Partial`].
    #[serde(rename_all = "camelCase")]
    Partial {
        /// Where the pipeline stopped.
        stopped_at: StopperDto,
    },
}

impl From<&Completeness> for CompletenessDto {
    fn from(value: &Completeness) -> Self {
        match value {
            Completeness::Complete => Self::Complete,
            Completeness::Partial { stopped_at } => Self::Partial {
                stopped_at: stopped_at.into(),
            },
        }
    }
}

/// One row of a [`DefInspectionDto`]'s effective-field list. Mirrors
/// [`EffectiveField`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EffectiveFieldDto {
    /// The field's canonical path text (`FieldPath`'s `Display`).
    pub path: String,
    /// Indentation depth: the path's segment count minus one.
    pub depth: usize,
    /// Whether this field is a `li` list item, as opposed to a named leaf.
    pub is_list_item: bool,
    /// The field's rendered value; `null` for an empty leaf.
    pub value: Option<String>,
    /// Which stage last set it.
    pub provenance: ProvenanceDto,
}

impl From<&EffectiveField> for EffectiveFieldDto {
    fn from(value: &EffectiveField) -> Self {
        Self {
            path: value.path.to_string(),
            depth: value.depth,
            is_list_item: value.is_list_item,
            value: value.value.clone(),
            provenance: (&value.provenance).into(),
        }
    }
}

/// A `ParentName` chain link, or a template's direct child — both an
/// owner plus a def/template ref, so either can be rendered as a link to
/// the def page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefLinkDto {
    /// The linked def/template's canonical ref text.
    pub def_ref: String,
    /// Its `(def_type, def_name)`.
    pub key: DefKeyDto,
    /// The mod that owns/registers it.
    pub owner: String,
}

/// One finding naming the inspected def/template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefFindingDto {
    /// The finding's canonical key text.
    pub key: String,
    /// Whether it still needs the user's attention.
    pub status: ResolutionStatusDto,
}

/// Surfaced on [`DefInspectionDto::template_ambiguity`] whenever the
/// inspected `[@Name]` template has more than one registrant. Mirrors
/// [`TemplateAmbiguity`] — see that type's own doc comment for why there
/// is no single "winner" for a duplicated template `Name`, only a
/// per-child answer, the same one `apps/cli`'s `defs inspect` surfaces
/// (`TemplateAmbiguityJson`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TemplateAmbiguityDto {
    /// Every mod registering this `Name`, in load order.
    pub registrants: Vec<String>,
    /// One entry per known child, keyed by the child's own mod id, naming
    /// which registrant it actually resolves to under the selected order.
    pub resolutions: BTreeMap<String, String>,
}

impl From<&TemplateAmbiguity> for TemplateAmbiguityDto {
    fn from(value: &TemplateAmbiguity) -> Self {
        Self {
            registrants: value.registrants.iter().map(ToString::to_string).collect(),
            resolutions: value
                .resolutions
                .iter()
                .map(|(child, resolved_to)| (child.to_string(), resolved_to.to_string()))
                .collect(),
        }
    }
}

/// `inspect_def`'s result: everything the def page needs. Mirrors
/// [`DefInspection`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefInspectionDto {
    /// The ref this inspection was built for, echoed back verbatim.
    pub def_ref: String,
    /// Which order this inspection was computed against.
    pub source: OrderSourceDto,
    /// Every owner/registrant in the selected order.
    pub owners: Vec<DefToucherDto>,
    /// The effective owner. A disclosed representative (the last-loaded
    /// registrant), not a real single answer, when
    /// [`Self::template_ambiguity`] is `Some` — see that field's own doc
    /// comment.
    pub winner: String,
    /// `Some` only for a template with more than one registrant, where
    /// [`Self::winner`] above is a disclosed representative rather than a
    /// real answer.
    pub template_ambiguity: Option<TemplateAmbiguityDto>,
    /// Every mod whose patch ops target this def/template.
    pub patchers: Vec<PatcherDto>,
    /// The `ParentName` chain from the winner's raw node outward.
    pub parents: Vec<DefLinkDto>,
    /// Templates only: direct children.
    pub children: Vec<DefLinkDto>,
    /// Whether the effective-def pipeline completed.
    pub completeness: CompletenessDto,
    /// The pipeline's caveats, structured.
    pub caveats: Vec<CaveatDto>,
    /// The requested page of effective-field rows.
    pub fields: Vec<EffectiveFieldDto>,
    /// Fields matching [`EffectiveFieldFilterDto`], before paging.
    pub fields_total: usize,
    /// The effective def, rendered back to XML text.
    pub resolved_xml: String,
    /// Every finding naming this def/template, with its current status.
    pub findings: Vec<DefFindingDto>,
}

/// A `ParentName` chain link (winner outward): always a template, so
/// always addressed by [`Selector::NameAttr`].
fn template_link(key: &DefKey, owner: &ModId) -> DefLinkDto {
    DefLinkDto {
        def_ref: DefRef::new(key.clone(), Selector::NameAttr).to_string(),
        key: key.clone().into(),
        owner: owner.as_str().to_string(),
    }
}

/// A template's direct child, addressed by whichever selector
/// [`DefInspection::children`] already resolved for it —
/// `rim_session::use_cases::inspect_def::child_ref` picks
/// [`Selector::NameAttr`] for a child that is itself another abstract
/// template (no `owners_by_def` entry of its own) and [`Selector::DefName`]
/// for an ordinary concrete def, so this never has to guess (a
/// nested-template child linked as `DefName` would be a dead link).
fn child_link(owner: &ModId, def_ref: &DefRef) -> DefLinkDto {
    DefLinkDto {
        def_ref: def_ref.to_string(),
        key: def_ref.key.clone().into(),
        owner: owner.as_str().to_string(),
    }
}

/// Builds a [`DefInspectionDto`] from a [`DefInspection`], filtering and
/// paging its effective-field list per `filter`.
#[must_use]
pub fn build_def_inspection_dto(
    inspection: &DefInspection,
    filter: &EffectiveFieldFilterDto,
) -> DefInspectionDto {
    let page = rim_session::effective_fields::page(&inspection.effective, &filter.into());
    let fields = page.items.iter().map(Into::into).collect();
    let fields_total = page.total;
    DefInspectionDto {
        def_ref: inspection.def_ref.to_string(),
        source: inspection.source.into(),
        owners: inspection.owners.iter().map(Into::into).collect(),
        winner: inspection.winner.as_str().to_string(),
        template_ambiguity: inspection.template_ambiguity.as_ref().map(Into::into),
        patchers: inspection.patchers.iter().map(Into::into).collect(),
        parents: inspection
            .parents
            .iter()
            .map(|(key, owner)| template_link(key, owner))
            .collect(),
        children: inspection
            .children
            .iter()
            .map(|(owner, def_ref)| child_link(owner, def_ref))
            .collect(),
        completeness: (&inspection.effective.completeness).into(),
        caveats: inspection
            .effective
            .caveats
            .iter()
            .map(CaveatDto::from)
            .collect(),
        fields,
        fields_total,
        resolved_xml: inspection.resolved_xml.clone(),
        findings: inspection
            .findings
            .iter()
            .map(|(key, status)| DefFindingDto {
                key: key.to_string(),
                status: (*status).into(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::FieldPath;

    use super::*;

    #[test]
    fn change_kind_dto_round_trips_every_variant() {
        let kinds = [
            ChangeKind::OwnsDef,
            ChangeKind::OwnsTemplate,
            ChangeKind::PatchesDef,
            ChangeKind::OverridesAsset(AssetKind::Texture),
            ChangeKind::OverridesAsset(AssetKind::Sound),
            ChangeKind::OverridesAsset(AssetKind::KeyedTranslation),
        ];
        for kind in kinds {
            let dto: ChangeKindDto = kind.into();
            let back: ChangeKind = dto.into();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn change_kind_counts_dto_maps_each_bucket() {
        let mut counts = BTreeMap::new();
        counts.insert(ChangeKind::OwnsDef, 3);
        counts.insert(ChangeKind::OverridesAsset(AssetKind::Sound), 1);

        let dto: ChangeKindCountsDto = (&counts).into();

        assert_eq!(dto.owns_def, 3);
        assert_eq!(dto.overrides_sound, 1);
        assert_eq!(dto.owns_template, 0);
    }

    #[test]
    fn effective_field_filter_dto_maps_every_field() {
        let dto = EffectiveFieldFilterDto {
            only_patched: true,
            offset: 5,
            limit: 50,
        };

        let filter: EffectiveFieldFilter = (&dto).into();

        assert!(filter.only_patched);
        assert_eq!(filter.offset, 5);
        assert_eq!(filter.limit, 50);
    }

    #[test]
    fn effective_field_dto_renders_a_text_leafs_value_and_provenance() {
        let path: FieldPath = "label".parse().expect("valid field path");
        let field = EffectiveField {
            path,
            depth: 1,
            is_list_item: false,
            value: Some("a wall".to_string()),
            provenance: Provenance::Patch {
                mod_id: ModId::new("patcher.mod"),
                op_index: 0,
            },
        };

        let dto: EffectiveFieldDto = (&field).into();

        assert_eq!(dto.path, "label");
        assert_eq!(dto.depth, 1);
        assert!(!dto.is_list_item);
        assert_eq!(dto.value.as_deref(), Some("a wall"));
        assert_eq!(
            dto.provenance,
            ProvenanceDto::Patch {
                mod_id: "patcher.mod".to_string(),
                op_index: 0,
            }
        );
    }

    #[test]
    fn template_ambiguity_dto_maps_registrants_and_resolutions() {
        let ambiguity = TemplateAmbiguity {
            registrants: vec![ModId::new("first.mod"), ModId::new("second.mod")],
            resolutions: BTreeMap::from([(ModId::new("child.mod"), ModId::new("second.mod"))]),
        };

        let dto: TemplateAmbiguityDto = (&ambiguity).into();

        assert_eq!(dto.registrants, vec!["first.mod", "second.mod"]);
        assert_eq!(
            dto.resolutions.get("child.mod").map(String::as_str),
            Some("second.mod")
        );
    }

    #[test]
    fn template_link_uses_the_name_attr_selector() {
        let key = DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Base".to_string(),
        };
        let link = template_link(&key, &ModId::new("owner.mod"));
        assert_eq!(link.def_ref, "ThingDef/@Base");
        assert_eq!(link.owner, "owner.mod");
    }

    #[test]
    fn child_link_uses_the_def_name_selector_for_a_concrete_def() {
        let def_ref = DefRef::new(
            DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Reinforced".to_string(),
            },
            Selector::DefName,
        );
        let link = child_link(&ModId::new("child.mod"), &def_ref);
        assert_eq!(link.def_ref, "ThingDef/Reinforced");
    }

    #[test]
    fn child_link_uses_the_name_attr_selector_for_an_abstract_template() {
        let def_ref = DefRef::new(
            DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "ReinforcedBase".to_string(),
            },
            Selector::NameAttr,
        );
        let link = child_link(&ModId::new("child.mod"), &def_ref);
        assert_eq!(link.def_ref, "ThingDef/@ReinforcedBase");
    }
}
