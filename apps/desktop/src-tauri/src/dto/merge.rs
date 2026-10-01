//! DTOs for the merge editor: `get_merge_preview`, `set_merge_choices`,
//! `get_merge_mod`, `preview_merge_mod_file`.
//!
//! Filtering, paging, and totals over a cached [`rim_session::MergePreview`]'s
//! field diff live in `rim-session` (`MergePreview::field_page`, reached
//! through `Session::merge_fields`) per `apps/desktop/CLAUDE.md`
//! ("filtering, paging, and aggregation live in rim-session"). This
//! module only maps already-filtered/paged domain data to DTOs — pure
//! value rendering, no session access.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_merge::diff::{DiffClass, EntryKind, FieldDiff, Value};
use rim_merge::plan::Caveat;
use rim_resolve::domain::{FieldPath, FindingKey, MergeChoice, MergeState};
use rim_session::use_cases::{MergeEntryKind, MergeModEntry, MergeModRender};
use rim_session::{MergeFieldPage, MergeFieldTotals, MergePreview};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::DefKeyDto;
use super::finding::MergeChoiceDto;

/// Request shape for `get_merge_preview`'s field-list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeFieldFilterDto {
    /// Keep only `Conflict`-classified rows.
    pub only_conflicts: bool,
    /// Keep only rows whose field path text contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to return, capped at [`rim_session::MAX_PAGE_SIZE`]
    /// by [`rim_session::MergePreview::field_page`].
    pub limit: usize,
}

impl From<&MergeFieldFilterDto> for rim_session::MergeFieldFilter {
    fn from(value: &MergeFieldFilterDto) -> Self {
        Self {
            only_conflicts: value.only_conflicts,
            search: value.search.clone(),
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// Request shape for `get_merge_preview`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergePreviewRequestDto {
    /// The finding's canonical key text.
    pub key: String,
    /// Filters and pages the field list.
    pub filter: MergeFieldFilterDto,
    /// When set, build the preview against that compat patch's own scoped
    /// findings and decisions instead of the profile's
    ///
    pub patch_id: Option<String>,
}

/// Request shape for `set_merge_choices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetMergeChoicesRequestDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The full choices map — the client owns it — keyed by each field's
    /// canonical path text (see [`FieldPath`]'s `Display`).
    pub choices: BTreeMap<String, MergeChoiceDto>,
    /// When set, store the decision on that compat patch's own decisions
    /// instead of the profile's.
    pub patch_id: Option<String>,
}

/// Request shape for `preview_merge_mod_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PreviewMergeModFileRequestDto {
    /// The rendered file's path, relative to the merge mod's own folder.
    pub relative_path: String,
}

/// What `preview_merge_mod_file` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeModFileDto {
    /// The file's literal text content.
    pub content: String,
}

/// Mirrors [`DiffClass`], without its payload — the differing owners
/// live in [`MergeFieldDto::changed_by`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DiffClassDto {
    /// See [`DiffClass::Unchanged`].
    Unchanged,
    /// See [`DiffClass::OneSided`].
    OneSided,
    /// See [`DiffClass::Agreeing`].
    Agreeing,
    /// See [`DiffClass::Conflict`].
    Conflict,
}

impl From<&DiffClass> for DiffClassDto {
    fn from(value: &DiffClass) -> Self {
        match value {
            DiffClass::Unchanged => Self::Unchanged,
            DiffClass::OneSided { .. } => Self::OneSided,
            DiffClass::Agreeing { .. } => Self::Agreeing,
            DiffClass::Conflict { .. } => Self::Conflict,
        }
    }
}

/// Mirrors [`EntryKind`], without its `MapEntry` payload — the container's
/// own address lives in [`MergeFieldDto::container`] instead (a sibling
/// field rather than a struct variant keeps every `MergeFieldDto` reader
/// that only cares "is this a map entry" from having to match a payload
/// it doesn't need, matching how [`DiffClassDto`] already drops
/// [`DiffClass`]'s own payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EntryKindDto {
    /// See [`EntryKind::Leaf`].
    Leaf,
    /// See [`EntryKind::ListItem`].
    ListItem,
    /// See [`EntryKind::MapEntry`].
    MapEntry,
}

impl From<&EntryKind> for EntryKindDto {
    fn from(value: &EntryKind) -> Self {
        match value {
            EntryKind::Leaf => Self::Leaf,
            EntryKind::ListItem => Self::ListItem,
            EntryKind::MapEntry { .. } => Self::MapEntry,
        }
    }
}

/// [`MergeFieldDto::container`]: the keyed map's own address text when
/// `entry` is [`EntryKind::MapEntry`], `None` otherwise.
fn entry_container(entry: &EntryKind) -> Option<String> {
    match entry {
        EntryKind::MapEntry { container } => Some(container.to_string()),
        EntryKind::Leaf | EntryKind::ListItem => None,
    }
}

/// Mirrors [`MergeState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MergeStateDto {
    /// See [`MergeState::Complete`].
    #[serde(rename_all = "camelCase")]
    Complete {
        /// How many operations the resulting merge patch would contain.
        op_count: usize,
    },
    /// See [`MergeState::NeedsFieldInput`].
    NeedsFieldInput {
        /// How many fields still lack a choice.
        unresolved: usize,
        /// How many fields this def has in total.
        total: usize,
    },
    /// See [`MergeState::CannotMerge`].
    CannotMerge {
        /// Why, verbatim.
        reason: String,
    },
}

impl From<&MergeState> for MergeStateDto {
    fn from(value: &MergeState) -> Self {
        match value {
            MergeState::Complete { op_count } => Self::Complete {
                op_count: *op_count,
            },
            MergeState::NeedsFieldInput { unresolved, total } => Self::NeedsFieldInput {
                unresolved: *unresolved,
                total: *total,
            },
            MergeState::CannotMerge { reason } => Self::CannotMerge {
                reason: reason.clone(),
            },
        }
    }
}

/// The structural guard, surfaced for the
/// merge editor's own banner: which field triggered
/// it, and which owner's own contribution differs from the def's base
/// copy — see [`rim_merge::diff::StructuralChange`]. `null` on
/// [`MergePreviewDto`] whenever [`rim_session::MergePreview::structural_change`]
/// is `None` (always, for a `patchCollision` preview — the guard is
/// `defOverride`-only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeStructuralGuardDto {
    /// The triggering field's own display text, e.g. `"thingClass"` (see
    /// [`rim_merge::diff::StructuralField`]'s `Display` impl).
    pub field: String,
    /// The owner whose contribution differs from the def's base copy.
    pub by: String,
}

/// [`MergePreviewDto::structural_guard`]: `Some` naming the triggering
/// field/owner whenever `preview.structural_change` is set, `None`
/// otherwise.
fn structural_guard(preview: &MergePreview) -> Option<MergeStructuralGuardDto> {
    preview
        .structural_change
        .as_ref()
        .map(|change| MergeStructuralGuardDto {
            field: change.field.to_string(),
            by: change.by.as_str().to_string(),
        })
}

/// What kind of finding a [`MergePreviewDto`]/[`MergeModEntryDto`]
/// describes. Mirrors the two [`FindingKey`] shapes [`rim_session::use_cases::PlanMerge`]
/// accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MergeFindingKindDto {
    /// See [`FindingKey::DefOverride`].
    DefOverride,
    /// See [`FindingKey::PatchCollision`].
    PatchCollision,
}

/// Mirrors [`MergeEntryKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MergeEntryKindDto {
    /// See [`MergeEntryKind::DefOverride`].
    DefOverride,
    /// See [`MergeEntryKind::PatchCollision`].
    PatchCollision,
    /// See [`MergeEntryKind::Asset`].
    Asset,
}

impl From<MergeEntryKind> for MergeEntryKindDto {
    fn from(value: MergeEntryKind) -> Self {
        match value {
            MergeEntryKind::DefOverride => Self::DefOverride,
            MergeEntryKind::PatchCollision => Self::PatchCollision,
            MergeEntryKind::Asset => Self::Asset,
        }
    }
}

/// One owner in a [`MergePreviewDto`]'s selected-order owner list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeOwnerDto {
    /// The owner's mod id.
    pub mod_id: String,
    /// The owner's display name.
    pub name: String,
    /// The owner's 0-based position in the selected order.
    pub position: usize,
}

/// Row/field counts across a [`MergePreviewDto`]'s whole diff, before
/// filtering or paging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeTotalsDto {
    /// Every field the diff carries.
    pub fields: usize,
    /// `Unchanged` fields.
    pub unchanged: usize,
    /// `OneSided`/`Agreeing` fields — resolve automatically.
    pub auto: usize,
    /// `Conflict` fields.
    pub conflicts: usize,
    /// Fields the plan still lists as unresolved.
    pub unresolved: usize,
}

impl From<MergeFieldTotals> for MergeTotalsDto {
    fn from(value: MergeFieldTotals) -> Self {
        Self {
            fields: value.fields,
            unchanged: value.unchanged,
            auto: value.auto,
            conflicts: value.conflicts,
            unresolved: value.unresolved,
        }
    }
}

/// One field row in a [`MergePreviewDto`]'s field list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeFieldDto {
    /// The field's canonical path text (see [`FieldPath`]'s `Display`).
    pub path: String,
    /// Indentation depth: the path's segment count minus one.
    pub depth: usize,
    /// Whether this field is a `li` list item, as opposed to a named leaf.
    pub is_list_item: bool,
    /// Where this field sits, structurally — a plain leaf, a `li` list
    /// item, or a key of a tag-keyed map. `MergeValueCell`'s `isXml` rendering
    /// keys off `entry !== "leaf"`.
    pub entry: EntryKindDto,
    /// The keyed map's own address text, when [`Self::entry`] is
    /// [`EntryKindDto::MapEntry`] — `null` otherwise. Consecutive rows
    /// sharing a `container` are entries of the same map, grouped under
    /// one collapsible header by `MergeFieldTable`.
    pub container: Option<String>,
    /// How the candidates relate to the base value.
    pub class: DiffClassDto,
    /// Owners that differ from the base value, in selected order.
    pub changed_by: Vec<String>,
    /// 0..=100 — the confidence the editor pre-fills this field's auto
    /// result with.
    pub confidence: u8,
    /// The base owner's value; `null` when absent.
    pub base: Option<String>,
    /// Every owner's value, keyed by mod id; `null` when absent.
    pub candidates: BTreeMap<String, Option<String>>,
    /// What the plan currently produces for this field — the chosen
    /// value if one is stored, otherwise the automatic result (`null`
    /// for an unresolved `Conflict`).
    pub result: Option<String>,
    /// The stored choice for this field, if any.
    pub choice: Option<MergeChoiceDto>,
    /// For a `Conflict` row: the selected-order winner's mod id — a
    /// natural pre-selection for the editor's radio group, populated
    /// unconditionally (the def's winner still applies even when its own
    /// value isn't among the fields that disagree).
    pub preselected: Option<String>,
}

/// One finding's merge preview: the owner list, the field diff (filtered
/// and paged), and the resulting merge state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergePreviewDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The contested def.
    pub def_key: DefKeyDto,
    /// Which finding shape this preview is for.
    pub kind: MergeFindingKindDto,
    /// Every owner, in the selected order.
    pub owners: Vec<MergeOwnerDto>,
    /// The base owner's mod id.
    pub base: String,
    /// The winning owner's mod id.
    pub winner: String,
    /// Field counts across the whole diff, before filtering/paging.
    pub totals: MergeTotalsDto,
    /// How much of the diff still needs the user's input.
    pub state: MergeStateDto,
    /// The structural guard, when it forced
    /// [`Self::state`] to `needsFieldInput` — names the triggering field
    /// and the owner that differs, for the editor's one banner naming the
    /// field that triggered it. Always
    /// `null` for a `patchCollision` preview.
    pub structural_guard: Option<MergeStructuralGuardDto>,
    /// The merge plan's caveats, structured.
    pub caveats: Vec<CaveatDto>,
    /// The requested page of field rows, in the diff's own order.
    pub fields: Vec<MergeFieldDto>,
    /// Rows matching the filter, before paging.
    pub total: usize,
    /// The winner's node with the merge plan's fields applied, rendered as XML
    /// text — only for a `defOverride` preview whose `state` is
    /// `complete` (every field resolves; see
    /// [`rim_merge::plan::build_resolved_node`]'s contract). `null` for a
    /// `patchCollision` preview (its own single field row already shows
    /// what each side sets — see [`MergePreviewDto::fields`]) and for any
    /// preview that still needs input or can't merge.
    pub resolved_xml: Option<String>,
    /// Owners this preview's diff excluded because they're outside a
    /// patch's scope (the scoped participant rule) — always empty for a
    /// profile preview, and for a
    /// patch preview whenever its scope already covers every owner.
    pub out_of_scope_owners: Vec<String>,
    /// This finding's def/template ref (`FindingKey::def_ref()`, always
    /// `Some` for a `DefOverride`/`PatchCollision` key) — links the merge
    /// editor's header to the def inspector
    ///
    pub def_ref: String,
}

/// One mod named as a source of the generated merge mod's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeSourceModDto {
    /// The mod's id.
    pub mod_id: String,
    /// The mod's display name.
    pub name: String,
}

/// One `Merge`/`ShipAsset` decision's entry in [`MergeModDto::entries`].
/// Mirrors [`MergeModEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeModEntryDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The contested def; `null` for an `asset` entry.
    pub def_key: Option<DefKeyDto>,
    /// Which shape this entry is.
    pub kind: MergeEntryKindDto,
    /// The entry's merge state at render time.
    pub state: MergeStateDto,
    /// How many ops this entry contributed.
    pub op_count: usize,
    /// The mods whose values this entry's ops carry.
    pub depends_on: Vec<String>,
    /// The patch file this entry's ops land in, if any.
    pub patch_file: Option<String>,
    /// The structural guard: the field that
    /// forced this entry's `needsFieldInput` state, if any — always `null`
    /// for a `patchCollision`/`asset` entry, and for a `defOverride` entry
    /// the guard never fired for. The apply dialog uses this to describe a
    /// guarded skip ("not auto-merged — confirm the winner") rather than a
    /// genuine needs-input one.
    pub structural_guard_field: Option<String>,
}

impl From<&MergeModEntry> for MergeModEntryDto {
    fn from(entry: &MergeModEntry) -> Self {
        Self {
            key: entry.key.to_string(),
            def_key: entry.def_key.clone().map(DefKeyDto::from),
            kind: entry.kind.into(),
            state: (&entry.state).into(),
            op_count: entry.op_count,
            depends_on: entry
                .depends_on
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            patch_file: entry.patch_file.as_ref().map(|p| p.display().to_string()),
            structural_guard_field: entry.structural_guard_field.clone(),
        }
    }
}

/// The generated merge mod, described for the merge-mod page. Mirrors
/// [`MergeModRender`] plus `exists`, which the command reads off the
/// writer port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MergeModDto {
    /// The generated mod's derived `packageId`.
    pub package_id: String,
    /// The generated mod's folder name.
    pub folder_name: String,
    /// The generated mod's full path under the game's `Mods/` folder, for
    /// display.
    pub mods_path: String,
    /// Whether the folder currently exists on disk.
    pub exists: bool,
    /// One entry per `Merge`/`ShipAsset` decision.
    pub entries: Vec<MergeModEntryDto>,
    /// Relative paths [`render`] would produce — empty when nothing
    /// renders. This is a fresh preview render, not the one on disk: if
    /// `rimmerge.json` is among these paths, its `generatedAt` timestamp
    /// (see [`preview_merge_mod_file`]) is *this* render's own moment, not
    /// necessarily the one `apply` will actually write (which renders
    /// again, at its own time, when it runs).
    ///
    /// [`render`]: rim_merge::emit::render
    /// [`preview_merge_mod_file`]: crate::commands::merge::preview_merge_mod_file
    pub files: Vec<String>,
    /// Every mod whose content the generated mod actually carries.
    pub source_mods: Vec<MergeSourceModDto>,
}

/// Renders `value` the way the CLI's `merge plan` does (`Absent` -> a
/// leaf's text -> a list item's whole XML), but as `Option<String>`
/// (`Absent` -> `None`) rather than a display placeholder — this DTO
/// distinguishes "absent" from "empty string" with `null`, which a
/// terminal table can't.
#[must_use]
pub fn format_value(value: &Value) -> Option<String> {
    match value {
        Value::Absent => None,
        Value::Leaf(text) => Some(text.clone()),
        Value::Item(node) => Some(rim_merge::xml::render_node(node, 0).trim_end().to_string()),
    }
}

/// Mirrors [`Caveat`], structured — the frontend renders a localized
/// sentence from it (`utils/caveat.ts`'s `caveatLabel`) rather than
/// receiving prose built here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CaveatDto {
    /// See [`Caveat::DuplicateTemplate`].
    #[serde(rename_all = "camelCase")]
    DuplicateTemplate {
        /// The contested template name.
        name: String,
        /// Every mod that registered it, in load order.
        owners: Vec<String>,
    },
    /// See [`Caveat::ModSettingDefault`].
    #[serde(rename_all = "camelCase")]
    ModSettingDefault {
        /// The mod that shipped the operation.
        mod_id: String,
        /// The operation's `Class`.
        class: String,
    },
    /// See [`Caveat::FailedOp`].
    #[serde(rename_all = "camelCase")]
    FailedOp {
        /// The mod that shipped the operation.
        mod_id: String,
        /// The operation's xpath.
        xpath: String,
    },
    /// See [`Caveat::UnscopedOps`].
    #[serde(rename_all = "camelCase")]
    UnscopedOps {
        /// The mod.
        mod_id: String,
        /// How many such operations it has.
        count: usize,
    },
    /// See [`Caveat::MalformedOperation`].
    #[serde(rename_all = "camelCase")]
    MalformedOperation {
        /// The mod that shipped it.
        mod_id: String,
    },
    /// See [`Caveat::DefRemoved`].
    #[serde(rename_all = "camelCase")]
    DefRemoved {
        /// The mod whose `Remove` emptied the def.
        mod_id: String,
    },
    /// See [`Caveat::PositionalItem`].
    #[serde(rename_all = "camelCase")]
    PositionalItem {
        /// The item's address.
        path: String,
    },
    /// See [`Caveat::UnsafeXpathValue`].
    #[serde(rename_all = "camelCase")]
    UnsafeXpathValue {
        /// The field's address.
        path: String,
    },
    /// See [`Caveat::UnknownOwnerChoice`].
    #[serde(rename_all = "camelCase")]
    UnknownOwnerChoice {
        /// The field's address.
        path: String,
        /// The unrecognized mod.
        mod_id: String,
    },
    /// See [`Caveat::InvalidValueFragment`].
    #[serde(rename_all = "camelCase")]
    InvalidValueFragment {
        /// The field's address.
        path: String,
    },
    /// See [`Caveat::UnsupportedDrop`].
    #[serde(rename_all = "camelCase")]
    UnsupportedDrop {
        /// The field's address.
        path: String,
    },
    /// See [`Caveat::UnreconstructableChain`].
    #[serde(rename_all = "camelCase")]
    UnreconstructableChain {
        /// The field's address.
        path: String,
    },
    /// See [`Caveat::UnsettableLeaf`].
    #[serde(rename_all = "camelCase")]
    UnsettableLeaf {
        /// The field's address.
        path: String,
    },
    /// See [`Caveat::OutOfScopeOwners`].
    #[serde(rename_all = "camelCase")]
    OutOfScopeOwners {
        /// The owners outside the patch's scope that could still own the
        /// raw node this plan's ops target.
        mods: Vec<String>,
    },
    /// See [`Caveat::ClobberedMapEntry`].
    #[serde(rename_all = "camelCase")]
    ClobberedMapEntry {
        /// The dropped entry's own address.
        path: String,
        /// The mod whose contribution was clobbered.
        by: String,
    },
}

impl From<&Caveat> for CaveatDto {
    fn from(value: &Caveat) -> Self {
        match value {
            Caveat::DuplicateTemplate { name, owners } => Self::DuplicateTemplate {
                name: name.clone(),
                owners: owners.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Caveat::ModSettingDefault { mod_id, class } => Self::ModSettingDefault {
                mod_id: mod_id.as_str().to_string(),
                class: class.clone(),
            },
            Caveat::FailedOp { mod_id, xpath } => Self::FailedOp {
                mod_id: mod_id.as_str().to_string(),
                xpath: xpath.clone(),
            },
            Caveat::UnscopedOps { mod_id, count } => Self::UnscopedOps {
                mod_id: mod_id.as_str().to_string(),
                count: *count,
            },
            Caveat::MalformedOperation { mod_id } => Self::MalformedOperation {
                mod_id: mod_id.as_str().to_string(),
            },
            Caveat::DefRemoved { mod_id } => Self::DefRemoved {
                mod_id: mod_id.as_str().to_string(),
            },
            Caveat::PositionalItem { path } => Self::PositionalItem {
                path: path.to_string(),
            },
            Caveat::UnsafeXpathValue { path } => Self::UnsafeXpathValue {
                path: path.to_string(),
            },
            Caveat::UnknownOwnerChoice { path, mod_id } => Self::UnknownOwnerChoice {
                path: path.to_string(),
                mod_id: mod_id.as_str().to_string(),
            },
            Caveat::InvalidValueFragment { path } => Self::InvalidValueFragment {
                path: path.to_string(),
            },
            Caveat::UnsupportedDrop { path } => Self::UnsupportedDrop {
                path: path.to_string(),
            },
            Caveat::UnreconstructableChain { path } => Self::UnreconstructableChain {
                path: path.to_string(),
            },
            Caveat::UnsettableLeaf { path } => Self::UnsettableLeaf {
                path: path.to_string(),
            },
            Caveat::OutOfScopeOwners { mods } => Self::OutOfScopeOwners {
                mods: mods.iter().map(|id| id.as_str().to_string()).collect(),
            },
            Caveat::ClobberedMapEntry { path, by } => Self::ClobberedMapEntry {
                path: path.to_string(),
                by: by.as_str().to_string(),
            },
        }
    }
}

/// The differing owners for `class`, in `owners_order` — the ordering
/// [`MergeFieldDto::changed_by`] promises.
fn changed_by(class: &DiffClass, owners_order: &[ModId]) -> Vec<String> {
    let differing: std::collections::BTreeSet<&ModId> = match class {
        DiffClass::Unchanged => return Vec::new(),
        DiffClass::OneSided { by } => std::iter::once(by).collect(),
        DiffClass::Agreeing { by } | DiffClass::Conflict { by } => by.iter().collect(),
    };
    owners_order
        .iter()
        .filter(|id| differing.contains(id))
        .map(|id| id.as_str().to_string())
        .collect()
}

/// What the merge plan currently produces for `field` — the chosen
/// value's rendering if `choice` is stored, otherwise the diff class's
/// automatic result.
fn field_result(field: &FieldDiff, choice: Option<&MergeChoice>) -> Option<String> {
    if let Some(choice) = choice {
        return match choice {
            MergeChoice::From { mod_id } => field.candidates.get(mod_id).and_then(format_value),
            MergeChoice::Value { text } => Some(text.clone()),
            MergeChoice::Drop => None,
        };
    }
    match &field.class {
        DiffClass::Unchanged => format_value(&field.base),
        DiffClass::OneSided { by } => field.candidates.get(by).and_then(format_value),
        DiffClass::Agreeing { by } => by
            .iter()
            .next()
            .and_then(|id| field.candidates.get(id))
            .and_then(format_value),
        DiffClass::Conflict { .. } => None,
    }
}

/// The selected-order winner's id, for every `Conflict` row — `None` for
/// every other class, which either has an automatic result already or (for
/// `Unchanged`) needs no choice at all.
fn preselected(class: &DiffClass, winner: &ModId) -> Option<String> {
    match class {
        DiffClass::Conflict { .. } => Some(winner.as_str().to_string()),
        _ => None,
    }
}

fn build_field_dto(
    field: &FieldDiff,
    owners_order: &[ModId],
    winner: &ModId,
    choice: Option<&MergeChoice>,
) -> MergeFieldDto {
    MergeFieldDto {
        path: field.path.to_string(),
        depth: field.path.segments().len().saturating_sub(1),
        is_list_item: field.is_list_item,
        entry: (&field.entry).into(),
        container: entry_container(&field.entry),
        class: (&field.class).into(),
        changed_by: changed_by(&field.class, owners_order),
        confidence: field.class.confidence().percent(),
        base: format_value(&field.base),
        candidates: field
            .candidates
            .iter()
            .map(|(id, value)| (id.as_str().to_string(), format_value(value)))
            .collect(),
        result: field_result(field, choice),
        choice: choice.cloned().map(MergeChoiceDto::from),
        preselected: preselected(&field.class, winner),
    }
}

/// [`MergePreviewDto::resolved_xml`]: only for a `defOverride` preview
/// that's `MergeState::Complete` — a `patchCollision`'s single field row
/// already says what each side sets, and an incomplete/unmergeable
/// preview has no full result to show yet. Reuses
/// [`rim_merge::plan::build_resolved_node`] (itself built from the same
/// per-field resolution the merge plan's own ops come from, see that function's
/// doc comment for why it's not a second xpath-based applier) plus
/// [`rim_merge::xml::render_node`], the same renderer [`format_value`]
/// already uses for a single item's XML.
fn resolved_xml(
    preview: &MergePreview,
    kind: MergeFindingKindDto,
    def_type: &str,
    choices: &BTreeMap<FieldPath, MergeChoice>,
) -> Option<String> {
    if kind != MergeFindingKindDto::DefOverride {
        return None;
    }
    if !matches!(preview.state, MergeState::Complete { .. }) {
        return None;
    }
    let node = rim_merge::plan::build_resolved_node(def_type, &preview.diff, choices);
    Some(rim_merge::xml::render_node(&node, 0).trim_end().to_string())
}

/// The out-of-scope owners a patch's scoped participant rule excluded
/// from `preview`'s diff, if any — [`rim_merge::plan::MergePlan::caveats`]
/// carries at most one [`Caveat::OutOfScopeOwners`] (`PlanMerge` pushes it
/// once, if at all; see its own tests).
fn out_of_scope_owners(preview: &MergePreview) -> Vec<String> {
    preview
        .plan
        .caveats
        .iter()
        .find_map(|caveat| match caveat {
            Caveat::OutOfScopeOwners { mods } => {
                Some(mods.iter().map(|id| id.as_str().to_string()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// The two [`FindingKey`] shapes a merge preview can be built for, the
/// [`DefKey`] each carries, and the [`DefRef`] `key.def_ref()` resolves
/// for it — the inverse of what [`rim_session::use_cases::PlanMerge`]
/// itself accepts. Bundling all three here (rather than a bare
/// `(kind, DefKey)` plus a second, separate `key.def_ref()` call at
/// [`build_preview_dto`]'s own call site) means that function's single
/// "not a mergeable finding key" panic covers both — `DefOverride`/
/// `PatchCollision` are exactly the two variants `FindingKey::def_ref()`
/// itself resolves to `Some` for, so reusing its own answer here also
/// keeps this function from re-deriving the same `DefRef` a second, less
/// tested way.
///
/// [`DefKey`]: rim_resolve::domain::DefKey
/// [`DefRef`]: rim_resolve::domain::DefRef
fn finding_kind_and_def_key(
    key: &FindingKey,
) -> Option<(
    MergeFindingKindDto,
    rim_resolve::domain::DefKey,
    rim_resolve::domain::DefRef,
)> {
    let def_ref = key.def_ref()?;
    match key {
        FindingKey::DefOverride { key: def_key, .. } => {
            Some((MergeFindingKindDto::DefOverride, def_key.clone(), def_ref))
        }
        FindingKey::PatchCollision { key: def_key, .. } => Some((
            MergeFindingKindDto::PatchCollision,
            def_key.clone(),
            def_ref,
        )),
        _ => None,
    }
}

/// Builds a [`MergePreviewDto`] from a cached [`MergePreview`] and the
/// [`MergeFieldPage`] [`rim_session::Session::merge_fields`] already
/// filtered and paged from it, plus the finding's stored choices (if any)
/// and a mod-id -> display-name map. `key` must be the same
/// `DefOverride`/`PatchCollision` finding `preview`/`page` were built for
/// — every caller reaches this only after
/// [`rim_session::use_cases::PlanMerge`] itself has already accepted
/// `key`, so that invariant always holds in practice.
///
/// # Panics
///
/// Panics if `key` is neither `DefOverride` nor `PatchCollision` — see
/// above.
#[must_use]
pub fn build_preview_dto(
    preview: &MergePreview,
    page: &MergeFieldPage<'_>,
    key: &FindingKey,
    choices: &BTreeMap<FieldPath, MergeChoice>,
    mod_names: &BTreeMap<ModId, String>,
) -> MergePreviewDto {
    let (kind, def_key, def_ref) = finding_kind_and_def_key(key)
        .unwrap_or_else(|| unreachable!("PlanMerge only ever accepts a mergeable finding key"));

    let owners = preview
        .owners
        .iter()
        .enumerate()
        .map(|(position, id)| MergeOwnerDto {
            mod_id: id.as_str().to_string(),
            name: mod_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.as_str().to_string()),
            position,
        })
        .collect();

    let fields: Vec<MergeFieldDto> = page
        .fields
        .iter()
        .copied()
        .map(|field| {
            build_field_dto(
                field,
                &preview.owners,
                &preview.winner,
                choices.get(&field.path),
            )
        })
        .collect();

    MergePreviewDto {
        key: key.to_string(),
        def_key: def_key.clone().into(),
        kind,
        owners,
        base: preview.base.as_str().to_string(),
        winner: preview.winner.as_str().to_string(),
        totals: page.totals.into(),
        state: (&preview.state).into(),
        structural_guard: structural_guard(preview),
        caveats: preview.plan.caveats.iter().map(CaveatDto::from).collect(),
        fields,
        total: page.total,
        resolved_xml: resolved_xml(preview, kind, &def_key.def_type, choices),
        out_of_scope_owners: out_of_scope_owners(preview),
        def_ref: def_ref.to_string(),
    }
}

/// Builds a [`MergeModDto`] from [`RenderMergeMod`]'s output — identity
/// and path included — plus `exists`, which only the writer port can
/// answer and so is still the command's own to check.
///
/// [`RenderMergeMod`]: rim_session::use_cases::RenderMergeMod
#[must_use]
pub fn build_merge_mod_dto(
    exists: bool,
    render: &MergeModRender,
    mod_names: &BTreeMap<ModId, String>,
) -> MergeModDto {
    let entries: Vec<MergeModEntryDto> =
        render.entries.iter().map(MergeModEntryDto::from).collect();

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

    // Every entry that actually contributed to the render (a complete
    // merge with ops, or a located asset — never a skipped merge) names
    // the mods its own content came from; the union of those is exactly
    // what About.xml's own `modDependencies` declares (see
    // `rim_merge::emit::render`'s internal `source_mods` map, which this
    // mirrors from the outside since that map isn't part of `RenderedMod`).
    let mut source_mod_ids: std::collections::BTreeSet<ModId> = std::collections::BTreeSet::new();
    for entry in &render.entries {
        if matches!(entry.state, MergeState::Complete { .. }) {
            source_mod_ids.extend(entry.depends_on.iter().cloned());
        }
    }
    let source_mods = source_mod_ids
        .into_iter()
        .map(|id| MergeSourceModDto {
            name: mod_names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| id.as_str().to_string()),
            mod_id: id.as_str().to_string(),
        })
        .collect();

    MergeModDto {
        package_id: render.identity.package_id.as_str().to_string(),
        folder_name: render.identity.folder_name.clone(),
        mods_path: render.mods_path.display().to_string(),
        exists,
        entries,
        files,
        source_mods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_value_maps_absent_to_none() {
        assert_eq!(format_value(&Value::Absent), None);
    }

    #[test]
    fn format_value_keeps_a_leafs_text_including_empty() {
        assert_eq!(
            format_value(&Value::Leaf(String::new())),
            Some(String::new())
        );
        assert_eq!(
            format_value(&Value::Leaf("hello".to_string())),
            Some("hello".to_string())
        );
    }

    #[test]
    fn caveat_dto_maps_clobbered_map_entry() {
        let dto: CaveatDto = (&Caveat::ClobberedMapEntry {
            path: "wildAnimals/XBM_Theropod"
                .parse()
                .expect("valid field path"),
            by: ModId::new("a.mod"),
        })
            .into();
        assert_eq!(
            dto,
            CaveatDto::ClobberedMapEntry {
                path: "wildAnimals/XBM_Theropod".to_string(),
                by: "a.mod".to_string(),
            }
        );
    }

    #[test]
    fn entry_kind_dto_maps_a_map_entry_and_carries_no_payload() {
        let container: FieldPath = "wildAnimals".parse().expect("valid field path");
        let dto: EntryKindDto = (&EntryKind::MapEntry {
            container: container.clone(),
        })
            .into();
        assert_eq!(dto, EntryKindDto::MapEntry);
        assert_eq!(
            entry_container(&EntryKind::MapEntry { container }),
            Some("wildAnimals".to_string())
        );
    }

    #[test]
    fn entry_kind_dto_maps_leaf_and_list_item_with_no_container() {
        assert_eq!(EntryKindDto::from(&EntryKind::Leaf), EntryKindDto::Leaf);
        assert_eq!(entry_container(&EntryKind::Leaf), None);
        assert_eq!(
            EntryKindDto::from(&EntryKind::ListItem),
            EntryKindDto::ListItem
        );
        assert_eq!(entry_container(&EntryKind::ListItem), None);
    }

    #[test]
    fn build_field_dto_carries_the_map_entrys_own_container() {
        let container: FieldPath = "wildAnimals".parse().expect("valid field path");
        let mut path_segments = container.segments().to_vec();
        path_segments.push(rim_merge::tree::PathSegment::Child("Cobra".to_string()));
        let field = FieldDiff {
            path: FieldPath::new(path_segments),
            base: Value::Leaf("0.2".to_string()),
            candidates: BTreeMap::from([(ModId::new("a.mod"), Value::Leaf("0.3".to_string()))]),
            class: DiffClass::OneSided {
                by: ModId::new("a.mod"),
            },
            is_list_item: false,
            entry: EntryKind::MapEntry {
                container: container.clone(),
            },
        };

        let dto = build_field_dto(&field, &[ModId::new("a.mod")], &ModId::new("a.mod"), None);

        assert_eq!(dto.entry, EntryKindDto::MapEntry);
        assert_eq!(dto.container.as_deref(), Some("wildAnimals"));
    }

    #[test]
    fn build_field_dto_leaves_container_null_for_an_ordinary_leaf() {
        let field = FieldDiff {
            path: "label".parse().expect("valid field path"),
            base: Value::Leaf("hi".to_string()),
            candidates: BTreeMap::new(),
            class: DiffClass::Unchanged,
            is_list_item: false,
            entry: EntryKind::Leaf,
        };

        let dto = build_field_dto(&field, &[], &ModId::new("a.mod"), None);

        assert_eq!(dto.entry, EntryKindDto::Leaf);
        assert_eq!(dto.container, None);
    }

    #[test]
    fn caveat_dto_maps_out_of_scope_owners() {
        let dto: CaveatDto = (&Caveat::OutOfScopeOwners {
            mods: vec![ModId::new("a.mod"), ModId::new("c.mod")],
        })
            .into();
        assert_eq!(
            dto,
            CaveatDto::OutOfScopeOwners {
                mods: vec!["a.mod".to_string(), "c.mod".to_string()],
            }
        );
    }

    #[test]
    fn diff_class_dto_serializes_as_camel_case() {
        let dto: DiffClassDto = (&DiffClass::OneSided {
            by: ModId::new("a.mod"),
        })
            .into();
        let json = serde_json::to_value(dto).expect("serializable");
        assert_eq!(json, "oneSided");
    }

    #[test]
    fn merge_state_dto_serializes_complete_with_camel_case_field() {
        let dto: MergeStateDto = (&MergeState::Complete { op_count: 3 }).into();
        let json = serde_json::to_value(&dto).expect("serializable");
        assert_eq!(json["kind"], "complete");
        assert_eq!(json["opCount"], 3);
    }

    /// A minimal, hand-built [`MergePreview`] whose only interesting field
    /// for [`structural_guard`]'s own tests is `structural_change` —
    /// mirrors `rim-session`'s own `render_merge_mod.rs` test literals
    /// (the diff/plan content is never read by `structural_guard`, so it's
    /// left empty).
    fn merge_preview_with_structural_change(
        structural_change: Option<rim_merge::diff::StructuralChange>,
    ) -> MergePreview {
        let base = ModId::new("core.mod");
        let winner = ModId::new("winner.mod");
        MergePreview {
            key: FindingKey::DefOverride {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Widget".to_string(),
                },
                owners: [base.clone(), winner.clone()].into_iter().collect(),
            },
            owners: vec![base.clone(), winner.clone()],
            base: base.clone(),
            winner: winner.clone(),
            diff: rim_merge::diff::ThreeWayDiff {
                base,
                fields: Vec::new(),
            },
            plan: rim_merge::plan::MergePlan {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Widget".to_string(),
                },
                selector: rim_analyzer::domain::Selector::DefName,
                winner,
                owners: Vec::new(),
                ops: Vec::new(),
                unresolved: Vec::new(),
                caveats: Vec::new(),
            },
            state: MergeState::NeedsFieldInput {
                unresolved: 1,
                total: 1,
            },
            structural_change,
            final_values: BTreeMap::new(),
        }
    }

    #[test]
    fn structural_guard_names_the_field_and_owner_when_the_preview_carries_one() {
        let preview =
            merge_preview_with_structural_change(Some(rim_merge::diff::StructuralChange {
                field: rim_merge::diff::StructuralField::ThingClass,
                by: ModId::new("winner.mod"),
            }));

        let guard = structural_guard(&preview).expect("a structural_change must surface a guard");
        assert_eq!(guard.field, "thingClass");
        assert_eq!(guard.by, "winner.mod");
    }

    #[test]
    fn structural_guard_is_none_when_the_preview_carries_no_structural_change() {
        let preview = merge_preview_with_structural_change(None);
        assert_eq!(structural_guard(&preview), None);
    }

    #[test]
    fn merge_mod_entry_dto_carries_the_structural_guard_field_through() {
        let entry = MergeModEntry {
            key: FindingKey::DefOverride {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Widget".to_string(),
                },
                owners: [ModId::new("core.mod"), ModId::new("winner.mod")]
                    .into_iter()
                    .collect(),
            },
            def_key: Some(rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Widget".to_string(),
            }),
            kind: MergeEntryKind::DefOverride,
            state: MergeState::NeedsFieldInput {
                unresolved: 1,
                total: 1,
            },
            op_count: 0,
            depends_on: std::collections::BTreeSet::new(),
            patch_file: None,
            structural_guard_field: Some("thingClass".to_string()),
        };

        let dto = MergeModEntryDto::from(&entry);

        assert_eq!(dto.structural_guard_field.as_deref(), Some("thingClass"));
    }

    #[test]
    fn changed_by_orders_by_the_selected_order_not_the_by_sets_own_order() {
        let a = ModId::new("a.mod");
        let b = ModId::new("b.mod");
        let owners_order = vec![b.clone(), a.clone()];
        let class = DiffClass::Conflict {
            by: [a.clone(), b.clone()].into_iter().collect(),
        };

        let result = changed_by(&class, &owners_order);

        assert_eq!(result, vec!["b.mod".to_string(), "a.mod".to_string()]);
    }

    #[test]
    fn preselected_is_always_the_winner_for_a_conflict_row_and_none_otherwise() {
        let winner = ModId::new("winner.mod");
        let other = ModId::new("other.mod");
        let conflict_with_winner = DiffClass::Conflict {
            by: [winner.clone(), other.clone()].into_iter().collect(),
        };
        let conflict_without_winner = DiffClass::Conflict {
            by: [other.clone()].into_iter().collect(),
        };

        assert_eq!(
            preselected(&conflict_with_winner, &winner),
            Some("winner.mod".to_string())
        );
        assert_eq!(
            preselected(&conflict_without_winner, &winner),
            Some("winner.mod".to_string()),
            "the winner is preselected even when it isn't one of the differing candidates"
        );
        assert_eq!(
            preselected(&DiffClass::Unchanged, &winner),
            None,
            "only Conflict rows ever get a preselection"
        );
    }

    #[test]
    fn merge_field_filter_dto_maps_every_field_into_the_domain_filter() {
        let dto = MergeFieldFilterDto {
            only_conflicts: true,
            search: Some("foo".to_string()),
            offset: 3,
            limit: 10,
        };

        let filter: rim_session::MergeFieldFilter = (&dto).into();

        assert!(filter.only_conflicts);
        assert_eq!(filter.search.as_deref(), Some("foo"));
        assert_eq!(filter.offset, 3);
        assert_eq!(filter.limit, 10);
    }

    #[test]
    fn merge_totals_dto_maps_from_merge_field_totals() {
        let totals = MergeFieldTotals {
            fields: 5,
            unchanged: 1,
            auto: 2,
            conflicts: 2,
            unresolved: 1,
        };

        let dto: MergeTotalsDto = totals.into();

        assert_eq!(dto.fields, 5);
        assert_eq!(dto.unchanged, 1);
        assert_eq!(dto.auto, 2);
        assert_eq!(dto.conflicts, 2);
        assert_eq!(dto.unresolved, 1);
    }

    /// A real preview, built through the actual `rim-merge` pipeline
    /// (xml parse -> inherit -> diff -> plan) via
    /// [`rim_session::use_cases::PlanMerge`] against
    /// `rim_session::test_support::bionic_heart_fixture_flat` — the guard-free
    /// twin of the worked BionicHeart def-override example, a real fixture
    /// exercised through the real pipeline, not a hand-rolled stand-in.
    /// Confirms the mapping end to end: owners/base/winner, state, and that a
    /// field only BIONICS's own template registration supplies
    /// (`defaultLabelColor`) survives into the DTO's field list. Uses the
    /// flat fixture, not `bionic_heart_fixture` itself: the real fixture's
    /// `ParentName` *string* differs between owners, which fires the
    /// structural guard and makes every `Complete`-only assertion here
    /// (`resolved_xml`, in particular) fail — this test is about the DTO
    /// mapping, not the guard (that gets its own coverage in `rim-session`'s
    /// own `plan_merge`/`session` test modules). The flat fixture keeps real
    /// inheritance content (see its own doc comment for how) specifically so
    /// this assertion — a field only reachable through template resolution,
    /// not either owner's own raw node — still means something.
    #[test]
    fn build_preview_dto_maps_a_real_preview_from_the_bionic_heart_fixture() {
        use rim_session::test_support::{bionic_heart_fixture_flat, session_with_sources};
        use rim_session::use_cases::PlanMerge;

        let fixture = bionic_heart_fixture_flat();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let key = FindingKey::DefOverride {
            key: rim_resolve::domain::DefKey {
                def_type: "HediffDef".to_string(),
                def_name: "BionicHeart".to_string(),
            },
            owners: [
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork"),
            ]
            .into_iter()
            .collect(),
        };
        let use_case = PlanMerge::new(fixture.reader);
        use_case
            .execute(&mut session, &key)
            .expect("planning must succeed");
        let preview = session
            .merge_preview(
                &rim_session::PreviewSlot::profile(rim_resolve::domain::OrderSource::Current),
                &key,
            )
            .expect("just planned above");

        let mod_names = BTreeMap::from([
            (ModId::new("ludeon.rimworld"), "RimWorld".to_string()),
            (
                ModId::new("example.bionicsfork"),
                "Example Bionics Fork".to_string(),
            ),
        ]);
        let filter = rim_session::MergeFieldFilter {
            only_conflicts: false,
            search: None,
            offset: 0,
            limit: 200,
        };
        let page = preview.field_page(&filter);

        let dto = build_preview_dto(preview, &page, &key, &BTreeMap::new(), &mod_names);

        assert_eq!(dto.key, key.to_string());
        assert_eq!(dto.kind, MergeFindingKindDto::DefOverride);
        assert_eq!(dto.base, "ludeon.rimworld");
        assert_eq!(dto.winner, "example.bionicsfork");
        assert_eq!(
            dto.owners,
            vec![
                MergeOwnerDto {
                    mod_id: "ludeon.rimworld".to_string(),
                    name: "RimWorld".to_string(),
                    position: 0,
                },
                MergeOwnerDto {
                    mod_id: "example.bionicsfork".to_string(),
                    name: "Example Bionics Fork".to_string(),
                    position: 1,
                },
            ]
        );
        assert!(matches!(dto.state, MergeStateDto::Complete { op_count: 0 }));
        assert_eq!(dto.total, dto.totals.fields, "no filter applied here");
        assert!(
            dto.fields.iter().any(|f| f.path == "defaultLabelColor"),
            "a field only BIONICS's own template registration supplies must still surface"
        );
        assert!(
            dto.fields.iter().all(|f| f.preselected.is_none()),
            "no field in this fixture is a real Conflict"
        );
        let resolved_xml = dto
            .resolved_xml
            .as_deref()
            .expect("a Complete defOverride preview always has a resolved XML");
        assert!(
            resolved_xml.contains("synthetic heart"),
            "the winner's changed leaf (label, OneSided by BIONICS) must appear: {resolved_xml}"
        );
        assert!(
            resolved_xml.contains("Hediff_AddedPart"),
            "the base's untouched leaf (hediffClass, Unchanged) must still appear: {resolved_xml}"
        );

        let only_conflicts_page = preview.field_page(&rim_session::MergeFieldFilter {
            only_conflicts: true,
            ..filter
        });
        let filtered = build_preview_dto(
            preview,
            &only_conflicts_page,
            &key,
            &BTreeMap::new(),
            &mod_names,
        );
        assert_eq!(
            filtered.total, 0,
            "every field in this fixture is OneSided, never Conflict"
        );
    }
}
