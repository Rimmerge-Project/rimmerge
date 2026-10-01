//! Row DTOs: targets, row values, rows and sections, and the row/section/item requests and results.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{AssignmentRow, FieldPath, RowKey, RowValue, Section, TargetRef};
use rim_session::use_cases::{
    CopyFromOutcome, DroppedItemSlotValue, ListItem, ListItemsFilter, ListItemsPage,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AssignmentDetailDto;
use super::schema::{AssignmentSchemaDto, parse_field_path};
use crate::dto::patch::ModRefDto;
use crate::error::CommandError;

/// Mirrors [`TargetRef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TargetRefDto {
    /// The [`FieldRole::TargetKey`](rim_resolve::domain::FieldRole::TargetKey) field's canonical path text.
    pub key_field: String,
    /// The target def itself.
    pub def: super::common::DefKeyDto,
}

impl From<&TargetRef> for TargetRefDto {
    fn from(value: &TargetRef) -> Self {
        Self {
            key_field: value.key_field.to_string(),
            def: value.def.clone().into(),
        }
    }
}

impl TryFrom<TargetRefDto> for TargetRef {
    type Error = CommandError;

    fn try_from(value: TargetRefDto) -> Result<Self, Self::Error> {
        Ok(Self {
            key_field: parse_field_path(&value.key_field)?,
            def: value.def.into(),
        })
    }
}

/// Mirrors [`RowValue`]; tagged `"kind"`, this DTO layer's own convention
/// for a value enum (matches [`super::finding::ActionDto`](crate::dto::finding::ActionDto)'s own tag
/// choice) rather than the domain's untagged serde shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RowValueDto {
    /// See [`RowValue::Names`].
    Names {
        /// Item names.
        names: Vec<String>,
    },
    /// See [`RowValue::Numbers`].
    Numbers {
        /// Chance values.
        numbers: Vec<f64>,
    },
    /// See [`RowValue::Text`].
    Text {
        /// The scalar's text.
        text: String,
    },
    /// See [`RowValue::Omit`].
    Omit,
}

impl From<RowValue> for RowValueDto {
    fn from(value: RowValue) -> Self {
        match value {
            RowValue::Names(names) => Self::Names { names },
            RowValue::Numbers(numbers) => Self::Numbers { numbers },
            RowValue::Text(text) => Self::Text { text },
            RowValue::Omit => Self::Omit,
        }
    }
}

impl From<RowValueDto> for RowValue {
    fn from(value: RowValueDto) -> Self {
        match value {
            RowValueDto::Names { names } => Self::Names(names),
            RowValueDto::Numbers { numbers } => Self::Numbers(numbers),
            RowValueDto::Text { text } => Self::Text(text),
            RowValueDto::Omit => Self::Omit,
        }
    }
}

/// `BTreeMap<FieldPath, RowValue>`, wrapped the same way [`FieldSpecMapDto`](super::schema::FieldSpecMapDto)
/// is (see that type's own doc comment for why this has no `#[ts(type =
/// "Record<...>")]` override).
// See `FieldSpecMapDto` for why there is no `#[serde(transparent)]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RowValueMapDto(pub BTreeMap<String, RowValueDto>);

impl From<&BTreeMap<FieldPath, RowValue>> for RowValueMapDto {
    fn from(value: &BTreeMap<FieldPath, RowValue>) -> Self {
        Self(
            value
                .iter()
                .map(|(path, value)| (path.to_string(), value.clone().into()))
                .collect(),
        )
    }
}

impl TryFrom<RowValueMapDto> for BTreeMap<FieldPath, RowValue> {
    type Error = CommandError;

    fn try_from(value: RowValueMapDto) -> Result<Self, Self::Error> {
        value
            .0
            .into_iter()
            .map(|(path, value)| Ok((parse_field_path(&path)?, value.into())))
            .collect()
    }
}

/// Mirrors [`AssignmentRow`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentRowDto {
    /// Per field: the value the user chose.
    pub values: RowValueMapDto,
    /// The emitted `defName`.
    pub def_name: String,
    /// A free-text note.
    pub note: Option<String>,
}

impl From<&AssignmentRow> for AssignmentRowDto {
    fn from(value: &AssignmentRow) -> Self {
        Self {
            values: (&value.values).into(),
            def_name: value.def_name.clone(),
            note: value.note.clone(),
        }
    }
}

impl TryFrom<AssignmentRowDto> for AssignmentRow {
    type Error = CommandError;

    fn try_from(value: AssignmentRowDto) -> Result<Self, Self::Error> {
        Ok(Self {
            values: value.values.try_into()?,
            def_name: value.def_name,
            note: value.note,
        })
    }
}

/// One `(target, row)` pair — [`AssignmentDetailDto::rows`]'s own shape:
/// a `Vec`, never a JSON map, since [`TargetRefDto`] is a struct
/// (mirrors [`rim_io`]'s own `StoredAssignmentProject` on-disk shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentRowEntryDto {
    /// The target this row is for.
    pub target: TargetRefDto,
    /// The row itself.
    pub row: AssignmentRowDto,
}

/// One free-standing ("new def") row —
/// [`AssignmentSectionDto::standalone_rows`]'s own shape, for a
/// standalone section ([`AssignmentSectionDto::is_standalone`]). A `Vec`
/// keyed by
/// nothing but the row's own `defName` — there is no target to pair it
/// with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StandaloneRowEntryDto {
    /// The row itself; `row.defName` is also this entry's own key.
    pub row: AssignmentRowDto,
}

// -- Summary / detail ---------------------------------------------------

/// Every mod in `ids` mapped to a [`ModRefDto`], sorted by id — the same
/// shape `dto::patch`'s `scope_ref_dtos` builds for a patch's scope.
/// `pub(crate)` so `commands::assignments` can build the same shape for
/// `list_assignment_candidates`'s `effectiveRefs`, which isn't tied to a
/// loaded [`AssignmentProject`].
pub(crate) fn mod_ref_dtos(
    ids: &BTreeSet<ModId>,
    names: &BTreeMap<ModId, String>,
) -> Vec<ModRefDto> {
    ids.iter()
        .map(|id| ModRefDto {
            mod_id: id.as_str().to_string(),
            name: names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.as_str().to_string()),
        })
        .collect()
}

/// One section (def type) of an [`AssignmentProject`](rim_resolve::domain::AssignmentProject),
/// mirroring `apps/cli`'s own `ShowJson`/`SectionJson`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentSectionDto {
    /// The section's own def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// The confirmed schema.
    pub schema: AssignmentSchemaDto,
    /// Whether this section has no `TargetKey` field at all — a "new def"
    /// section, whose rows are free-standing ([`Self::standalone_rows`])
    /// rather than target-addressed ([`Self::rows`], always empty in that
    /// case).
    pub is_standalone: bool,
    /// Every target this section has a row for. Always empty when
    /// [`Self::is_standalone`].
    pub rows: Vec<AssignmentRowEntryDto>,
    /// Every free-standing row. Always empty otherwise.
    pub standalone_rows: Vec<StandaloneRowEntryDto>,
}

/// Builds one [`AssignmentSectionDto`] from a loaded [`Section`].
pub(super) fn section_dto(def_type: &str, section: &Section) -> AssignmentSectionDto {
    AssignmentSectionDto {
        def_type: def_type.to_string(),
        schema: (&section.schema).into(),
        is_standalone: section.is_standalone(),
        rows: section
            .rows
            .iter()
            .filter_map(|(key, row)| match key {
                RowKey::Target(target) => Some(AssignmentRowEntryDto {
                    target: target.into(),
                    row: row.into(),
                }),
                RowKey::Own(_) => None,
            })
            .collect(),
        standalone_rows: section
            .rows
            .iter()
            .filter_map(|(key, row)| match key {
                RowKey::Own(_) => Some(StandaloneRowEntryDto { row: row.into() }),
                RowKey::Target(_) => None,
            })
            .collect(),
    }
}

/// Request shape for `set_assignment_row`. Addresses a target-keyed row
/// (`target` given) or a free-standing one (`target` omitted, addressed
/// by `row.defName` alone) — one merged shape for both kinds of row,
/// mirroring `apps/cli`'s own merged `set-row`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetAssignmentRowRequestDto {
    /// Which project to set a row on.
    pub assignment_id: String,
    /// Which section (by def type) to address — optional when the
    /// project has exactly one section; erroring, not guessing, once it
    /// has more than one.
    pub section: Option<String>,
    /// The target this row is for. Omit for a free-standing row.
    pub target: Option<TargetRefDto>,
    /// The row itself.
    pub row: AssignmentRowDto,
}

/// What `set_assignment_row`/`clear_assignment_row` return: the row this
/// call replaced, or `null` when the target had none before.
///
/// `copy_assignment_row_from` never returned this DTO despite an earlier
/// version of this doc comment claiming it did — it builds a fresh,
/// unpersisted row for the editor to keep filling in, never replacing
/// anything, so "the row this call replaced" never applied to it; see
/// [`CopyAssignmentRowResultDto`] for what it actually returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentRowResultDto {
    /// The row this call replaced, if the target already had one.
    pub replaced: Option<AssignmentRowDto>,
}

/// One [`DroppedItemSlotValue`] transcribed for the wire — mirrors
/// `apps/cli`'s own `DroppedItemSlotValueJson`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DroppedItemSlotValueDto {
    /// The item-slot field the value was dropped from.
    pub path: String,
    /// The slot's own item type.
    pub def_type: String,
    /// The unknown name that was dropped.
    pub name: String,
}

impl From<&DroppedItemSlotValue> for DroppedItemSlotValueDto {
    fn from(value: &DroppedItemSlotValue) -> Self {
        Self {
            path: value.path.to_string(),
            def_type: value.def_type.clone(),
            name: value.name.clone(),
        }
    }
}

/// What `copy_assignment_row_from` returns: the freshly built row (still
/// needing `set_assignment_row` to validate and persist it) plus every
/// borrowed `ItemSlot` value it had to drop because the def it named
/// isn't active — mirrors `apps/cli`'s own `CopyFromRowJson`. `dropped`
/// is surfaced rather than discarded, so this list — real, reported by
/// [`CopyFromOutcome::dropped`] and rendered by the CLI too — never
/// silently vanishes between the use case and the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CopyAssignmentRowResultDto {
    /// The freshly built row.
    pub row: AssignmentRowDto,
    /// Every borrowed value dropped because it named a def that isn't
    /// active.
    pub dropped: Vec<DroppedItemSlotValueDto>,
}

impl From<&CopyFromOutcome> for CopyAssignmentRowResultDto {
    fn from(value: &CopyFromOutcome) -> Self {
        Self {
            row: (&value.row).into(),
            dropped: value.dropped.iter().map(Into::into).collect(),
        }
    }
}

/// Request shape for `clear_assignment_row`. Addresses a target-keyed row
/// (`target` given) or a free-standing one (`def_name` given) — exactly
/// one of the two, mirroring [`SetAssignmentRowRequestDto`] and
/// `apps/cli`'s own merged `clear-row`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ClearAssignmentRowRequestDto {
    /// Which project to clear a row on.
    pub assignment_id: String,
    /// See [`SetAssignmentRowRequestDto::section`].
    pub section: Option<String>,
    /// The target whose row is cleared. Omit together with giving
    /// `defName` instead, for a free-standing row.
    pub target: Option<TargetRefDto>,
    /// A free-standing row's own `defName`. Required exactly when
    /// `target` is omitted.
    pub def_name: Option<String>,
}

// -- coverage -------------------------------------------------------------

/// Request shape for `list_assignment_items`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListAssignmentItemsRequestDto {
    /// The item type to list, e.g. `example.PartDef`.
    pub def_type: String,
    /// Search and paging.
    pub filter: ListItemsFilterDto,
    /// An assignment project whose own free-standing rows of this exact
    /// item type (if it has a section for one) are prepended, labelled
    /// `own: true` — mirrors `apps/cli`'s own `items --assignment`. No
    /// separate `section` field: the lookup is always against this same
    /// `defType`, never a different section chosen by the caller.
    pub assignment_id: Option<String>,
}

/// Mirrors [`ListItemsFilter`]. `limit` is capped server-side at
/// [`rim_session::MAX_PAGE_SIZE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListItemsFilterDto {
    /// Only defs whose `defName` contains this substring (case-insensitive).
    pub search: Option<String>,
    /// How many matching defs to skip.
    pub offset: usize,
    /// How many defs to return, capped at [`rim_session::MAX_PAGE_SIZE`].
    pub limit: usize,
}

impl From<ListItemsFilterDto> for ListItemsFilter {
    fn from(value: ListItemsFilterDto) -> Self {
        Self {
            search: value.search,
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// Mirrors [`ListItem`]. `own` distinguishes the project's own
/// free-standing rows (prepended by [`ListAssignmentItemsRequestDto::assignment_id`])
/// from the active list's own matches, so the picker can render a "this
/// project" group first without guessing from [`Self::owner`] — a real
/// active def's owner happens to never be the project's own package id,
/// but that's incidental, never the contract (mirroring `apps/cli`'s own `--json` `own` field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListItemDto {
    /// The item's own type and `defName`.
    pub def: super::common::DefKeyDto,
    /// The mod whose copy wins under the selected order — the project's
    /// own package id when [`Self::own`].
    pub owner: String,
    /// The first top-level scalar child's text, `defName` excluded.
    pub hint: Option<String>,
    /// Whether this item is one of the project's own free-standing rows,
    /// rather than an active def.
    pub own: bool,
}

/// Builds one [`ListItemDto`], deciding [`ListItemDto::own`] by checking
/// `item`'s own `defName` against `own_names` — never by comparing
/// `item.owner` against the project's own package id (see
/// [`ListItemDto`]'s own doc comment for why).
fn list_item_dto(item: &ListItem, own_names: &BTreeSet<String>) -> ListItemDto {
    ListItemDto {
        def: item.def.clone().into(),
        owner: item.owner.as_str().to_string(),
        hint: item.hint.clone(),
        own: own_names.contains(&item.def.def_name),
    }
}

/// Mirrors [`ListItemsPage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListItemsPageDto {
    /// Total defs of the type matching the filter, before paging.
    pub total: usize,
    /// This page's items, in `defName` order.
    pub items: Vec<ListItemDto>,
}

/// Builds a [`ListItemsPageDto`], deciding each item's own [`ListItemDto::own`]
/// against `own_names` (the addressed project's own free-standing row
/// names of this def type, if any — empty when no project was given or it
/// has no section of this type).
#[must_use]
pub fn list_items_page_dto(
    value: &ListItemsPage,
    own_names: &BTreeSet<String>,
) -> ListItemsPageDto {
    ListItemsPageDto {
        total: value.total,
        items: value
            .items
            .iter()
            .map(|item| list_item_dto(item, own_names))
            .collect(),
    }
}

// -- copy_assignment_row_from --------------------------------------------

/// Request shape for `copy_assignment_row_from`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct CopyAssignmentRowFromRequestDto {
    /// Which project to build a row for.
    pub assignment_id: String,
    /// See [`SetAssignmentRowRequestDto::section`].
    pub section: Option<String>,
    /// The target the fresh row is built for.
    pub target: TargetRefDto,
    /// An existing instance of this project's own assignment def type to
    /// copy fields from, by its own `defName`.
    pub source_def_name: String,
}

// -- add_assignment_section / remove_assignment_section ------------------

/// Request shape for `add_assignment_section`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AddAssignmentSectionRequestDto {
    /// Which project to add a section to.
    pub assignment_id: String,
    /// The def type to infer and add as a new section.
    pub def_type: String,
}

/// Request shape for `remove_assignment_section`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RemoveAssignmentSectionRequestDto {
    /// Which project to remove a section from.
    pub assignment_id: String,
    /// The section's def type to remove.
    pub def_type: String,
    /// Remove the section even if another section's row still references
    /// one of its own free-standing rows, leaving that reference
    /// dangling (a later `export` reports it as a skip).
    pub force: bool,
}

/// What `remove_assignment_section` returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RemoveAssignmentSectionResultDto {
    /// Whether a section for this def type actually existed (and was
    /// removed) — idempotent, mirroring
    /// [`rim_session::use_cases::RemoveAssignmentSection`]'s own contract.
    pub removed: bool,
    /// The project's fresh detail.
    pub assignment: AssignmentDetailDto,
}

// -- export_assignment ----------------------------------------------------
