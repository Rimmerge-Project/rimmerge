//! DTOs for `list_working_mods`/`list_inactive_mods`: the Mods page's two
//! working-tier lists. Distinct from [`super::mods::ModSummaryDto`] (the
//! scanned tier's row): two DTOs may not share a Rust type name.

use rim_session::{
    GroupFilter, ListSort, ListSortKey, ModList, ModListQuery, ModListRow, RowState, SortDirection,
    UpdateTime, UpdateUnavailable,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::SourceDto;
use super::file_baseline::MembershipMarkDto;
use super::mods::GeneratedMarkerDto;
use super::organization::{ColorTokenDto, GroupLabelDto, parse_group_id};
use crate::error::CommandError;

/// Request shape for `list_working_mods`/`list_inactive_mods`. There is no
/// offset or limit: a list returns every matching row up to the backend's
/// own bound.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModListQueryDto {
    /// Only mods whose id or name contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// Only scanned mods carrying this tag (ignored by the Inactive list).
    pub tag: Option<String>,
    /// Only mods from this source.
    pub source: Option<SourceDto>,
    /// The display sort; `null` is the list's own default (load order for
    /// Active, name A to Z for Inactive).
    pub sort: Option<ListSortDto>,
    /// Only mods in this group scope; absent or `null` is no group filter.
    /// A filter, never a reorder.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub group: Option<GroupFilterDto>,
}

/// A group scope a list can be narrowed to. Mirrors [`GroupFilter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GroupFilterDto {
    /// Mods in this group or any group below it, each mod once.
    #[serde(rename_all = "camelCase")]
    InScope {
        /// The group.
        group_id: u32,
    },
    /// Mods that are in no group at all.
    Ungrouped,
}

impl TryFrom<GroupFilterDto> for GroupFilter {
    type Error = CommandError;

    fn try_from(value: GroupFilterDto) -> Result<Self, CommandError> {
        match value {
            GroupFilterDto::InScope { group_id } => Ok(Self::InScope(parse_group_id(group_id)?)),
            GroupFilterDto::Ungrouped => Ok(Self::Ungrouped),
        }
    }
}

/// What a list is ordered by. Mirrors [`ListSortKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ListSortKeyDto {
    /// The working set's own order (Active list only).
    LoadOrder,
    /// The display name.
    Name,
    /// The package id.
    PackageId,
    /// When the mod was last updated; unavailable times sort last.
    LastUpdated,
}

/// Which way a sort runs. Mirrors [`SortDirection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SortDirectionDto {
    /// Smallest / earliest / A first.
    Ascending,
    /// Largest / latest / Z first.
    Descending,
}

/// A list's display sort. Mirrors [`ListSort`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ListSortDto {
    /// What to order by.
    pub key: ListSortKeyDto,
    /// Which way.
    pub direction: SortDirectionDto,
}

impl From<ListSortDto> for ListSort {
    fn from(value: ListSortDto) -> Self {
        Self {
            key: match value.key {
                ListSortKeyDto::LoadOrder => ListSortKey::LoadOrder,
                ListSortKeyDto::Name => ListSortKey::Name,
                ListSortKeyDto::PackageId => ListSortKey::PackageId,
                ListSortKeyDto::LastUpdated => ListSortKey::LastUpdated,
            },
            direction: match value.direction {
                SortDirectionDto::Ascending => SortDirection::Ascending,
                SortDirectionDto::Descending => SortDirection::Descending,
            },
        }
    }
}

/// When a mod was last updated, or why that is unknown. Mirrors
/// [`UpdateTime`]; the reason is a code the interface words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UpdateTimeDto {
    /// The recorded time.
    Known {
        /// Seconds since the Unix epoch.
        #[ts(type = "number")]
        seconds: u64,
    },
    /// Core and the DLCs ship with the game.
    ShippedWithGame,
    /// No source could say.
    Unknown,
}

impl From<UpdateTime> for UpdateTimeDto {
    fn from(value: UpdateTime) -> Self {
        match value {
            UpdateTime::Known(seconds) => Self::Known {
                seconds: seconds.get(),
            },
            UpdateTime::Unavailable(UpdateUnavailable::ShippedWithGame) => Self::ShippedWithGame,
            UpdateTime::Unavailable(UpdateUnavailable::Unknown) => Self::Unknown,
        }
    }
}

impl TryFrom<ModListQueryDto> for ModListQuery {
    type Error = CommandError;

    fn try_from(value: ModListQueryDto) -> Result<Self, Self::Error> {
        Ok(Self {
            search: value.search,
            tag: value.tag.map(rim_resolve::domain::Tag::new).transpose()?,
            source: value.source.map(Into::into),
            sort: value.sort.map(Into::into),
            group: value.group.map(TryInto::try_into).transpose()?,
        })
    }
}

/// What a row's facts are known from. Mirrors [`RowState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModRowStateDto {
    /// Part of the last scan.
    #[serde(rename_all = "camelCase")]
    Scanned {
        /// Every tag the mod carries.
        tags: Vec<String>,
        /// Distinct active mods with a Hard-strength edge pointing at it.
        hard_dependents: usize,
    },
    /// No scan facts, only identity. In the Active list: activated since
    /// the last rescan. In the Inactive list: an ordinary inactive mod
    /// (inactive mods are never scanned).
    PendingScan,
    /// Listed in the working set but found nowhere on disk.
    Missing,
}

impl From<&RowState> for ModRowStateDto {
    fn from(value: &RowState) -> Self {
        match value {
            RowState::Scanned {
                tags,
                hard_dependents,
            } => Self::Scanned {
                tags: tags.iter().map(ToString::to_string).collect(),
                hard_dependents: *hard_dependents,
            },
            RowState::PendingScan => Self::PendingScan,
            RowState::Missing => Self::Missing,
        }
    }
}

/// One row of `list_working_mods`/`list_inactive_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModListRowDto {
    /// The mod's exact id.
    pub mod_id: String,
    /// Its display name — a missing row's own id text.
    pub name: String,
    /// Where its files come from; `null` only for a missing row.
    pub source: Option<SourceDto>,
    /// Its place in the working set (0-based, unfiltered); `null` in the
    /// Inactive list.
    pub position: Option<usize>,
    /// What is known about the row.
    pub state: ModRowStateDto,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarkerDto>,
    /// Its Steam Workshop published-file id, when it has one.
    #[ts(type = "number | null")]
    pub workshop_id: Option<u64>,
    /// When it was last updated, or why that is unknown.
    pub updated: UpdateTimeDto,
    /// How its membership differs from the file's active list, by base id.
    pub membership: MembershipMarkDto,
    /// The groups the mod is a direct member of, by name. Empty while
    /// organization is unavailable.
    pub groups: Vec<GroupLabelDto>,
    /// The mod's own colour; a group's colour is never inherited.
    pub color: Option<ColorTokenDto>,
}

impl From<&ModListRow> for ModListRowDto {
    fn from(value: &ModListRow) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            name: value.name.clone(),
            source: value.source.map(Into::into),
            position: value.position,
            state: (&value.state).into(),
            generated: value.generated.as_ref().map(Into::into),
            workshop_id: value.workshop_id,
            updated: value.updated.into(),
            membership: value.membership.into(),
            groups: value.groups.iter().map(Into::into).collect(),
            color: value.color.map(Into::into),
        }
    }
}

/// One list. `total` rows matched; `items` holds at most the backend's own
/// bound, so `total > items.length` means the list was cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModListDto {
    /// Rows matching the query before the bound.
    pub total: usize,
    /// The rows, in list order.
    pub items: Vec<ModListRowDto>,
    /// Whether the rows may be reordered through this view: the Active list
    /// in its own load order. A search or filter keeps it on (a move is
    /// anchored to a neighbour's id); another sort turns it off.
    pub reorderable: bool,
}

impl From<ModList> for ModListDto {
    fn from(value: ModList) -> Self {
        Self {
            total: value.total,
            items: value.items.iter().map(Into::into).collect(),
            reorderable: value.reorderable,
        }
    }
}
