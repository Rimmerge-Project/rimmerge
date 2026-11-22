//! DTOs for `list_working_mods`/`list_inactive_mods`: the Mods page's two
//! working-tier lists. Distinct from [`super::mods::ModSummaryDto`] (the
//! scanned tier's row): two DTOs may not share a Rust type name.

use std::ops::Range;

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
    /// (inactive mods are never scanned). Tags added by hand still show.
    #[serde(rename_all = "camelCase")]
    PendingScan {
        /// The tags added to the mod by hand.
        manual_tags: Vec<String>,
    },
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
            RowState::PendingScan { manual_tags } => Self::PendingScan {
                manual_tags: manual_tags.iter().map(ToString::to_string).collect(),
            },
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
    /// The groups the mod is a direct member of, in group-tree order. Empty while
    /// organization is unavailable.
    pub groups: Vec<GroupLabelDto>,
    /// The mod's own colour; a group's colour is never inherited.
    pub color: Option<ColorTokenDto>,
    /// Where the search matched in the name and in the id, so either can be
    /// marked whichever the list labels the row with. Absent without a
    /// search, and when the search matched neither text (a tag or source
    /// filter alone), which keeps a 10,000-row answer free of empty objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub search: Option<SearchHighlightDto>,
}

/// One matched part of a text, in UTF-16 code units (what JavaScript's
/// `String.prototype.slice` counts). `start` is inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TextRangeDto {
    /// The first code unit of the match.
    pub start: u32,
    /// One past the last code unit of the match.
    pub end: u32,
}

/// Where a search matched in a row's name and id, ascending and
/// non-overlapping, in UTF-16 code units of the text as sent. Mirrors
/// [`rim_session::SearchHighlight`], whose byte ranges are converted here, once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SearchHighlightDto {
    /// Matches in `name`.
    pub name: Vec<TextRangeDto>,
    /// Matches in `modId`.
    pub id: Vec<TextRangeDto>,
}

/// The byte `ranges` of `text` as UTF-16 ranges. A range that is not on
/// character boundaries of `text`, or does not fit a `u32`, is left out: the
/// backend never produces one, and a missing mark is better than a wrong one.
fn utf16_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<TextRangeDto> {
    ranges
        .iter()
        .filter_map(|range| {
            let before = text.get(..range.start)?;
            let marked = text.get(range.clone())?;
            let start = u32::try_from(before.encode_utf16().count()).ok()?;
            let length = u32::try_from(marked.encode_utf16().count()).ok()?;
            Some(TextRangeDto {
                start,
                end: start.checked_add(length)?,
            })
        })
        .collect()
}

impl SearchHighlightDto {
    /// The marks of `row`'s search, `None` when it matched nothing.
    fn of(row: &ModListRow) -> Option<Self> {
        let highlight = &row.search;
        (!highlight.is_empty()).then(|| Self {
            name: utf16_ranges(&row.name, &highlight.name),
            id: utf16_ranges(row.mod_id.as_str(), &highlight.id),
        })
    }
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
            search: SearchHighlightDto::of(value),
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

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_session::{MembershipMark, SearchHighlight, UpdateTime, UpdateUnavailable};

    use super::*;

    fn row(mod_id: &str, name: &str, search: SearchHighlight) -> ModListRow {
        ModListRow {
            mod_id: ModId::new(mod_id),
            name: name.to_string(),
            source: None,
            position: None,
            state: RowState::Missing,
            workshop_id: None,
            generated: None,
            updated: UpdateTime::Unavailable(UpdateUnavailable::Unknown),
            membership: MembershipMark::None,
            groups: Vec::new(),
            color: None,
            search,
        }
    }

    fn span(start: usize, end: usize) -> Range<usize> {
        start..end
    }

    fn range(start: u32, end: u32) -> TextRangeDto {
        TextRangeDto { start, end }
    }

    #[test]
    fn ascii_byte_ranges_are_the_same_utf16_ranges() {
        assert_eq!(
            utf16_ranges("Alpha Beta", &[0..5, 6..10]),
            [range(0, 5), range(6, 10)]
        );
    }

    #[test]
    fn a_multibyte_prefix_counts_code_units_not_bytes() {
        // "é" is 2 bytes and 1 unit, "日本" 6 bytes and 2 units: "mod" starts
        // at byte 8 but at unit 3.
        assert_eq!(utf16_ranges("é日本mod", &[span(8, 11)]), [range(3, 6)]);
    }

    #[test]
    fn a_character_outside_the_bmp_counts_two_code_units() {
        // U+1D7D8 is 4 bytes and a surrogate pair (2 units): "ab" starts at
        // byte 4 but at unit 2, and the JS slice [2, 4) is "ab".
        let text = "\u{1D7D8}ab";
        let converted = utf16_ranges(text, &[span(4, 6)]);
        assert_eq!(converted, [range(2, 4)]);
        let units: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(String::from_utf16_lossy(&units[2..4]), "ab");
    }

    #[test]
    fn a_marked_astral_character_spans_both_its_code_units() {
        assert_eq!(utf16_ranges("x\u{1D7D8}y", &[span(1, 5)]), [range(1, 3)]);
    }

    #[test]
    fn a_range_off_a_character_boundary_or_past_the_end_is_left_out() {
        // Byte 1 is inside the two-byte "é"; 9 is past the end.
        assert!(utf16_ranges("éa", &[1..2, 2..9]).is_empty());
        assert_eq!(utf16_ranges("éa", &[0..2, 1..3]), [range(0, 1)]);
    }

    #[test]
    fn a_row_converts_its_name_and_id_ranges_independently() {
        let search = SearchHighlight {
            name: vec![span(6, 9)],
            id: vec![span(0, 2)],
        };

        let dto = ModListRowDto::from(&row("ä.b", "日本Mod", search));

        let marks = dto.search.expect("a match is carried");
        assert_eq!(marks.name, [range(2, 5)]);
        assert_eq!(marks.id, [range(0, 1)]);
    }

    #[test]
    fn a_row_with_no_match_carries_no_search_and_serializes_without_the_key() {
        let dto = ModListRowDto::from(&row("a", "Alpha", SearchHighlight::default()));

        assert_eq!(dto.search, None);
        let json = serde_json::to_value(&dto).expect("serializes");
        assert!(json.get("search").is_none(), "{json}");
    }

    #[test]
    fn a_matched_row_serializes_camel_case_ranges() {
        let search = SearchHighlight {
            name: vec![span(0, 2)],
            id: Vec::new(),
        };

        let json = serde_json::to_value(ModListRowDto::from(&row("a", "Alpha", search)))
            .expect("serializes");

        assert_eq!(
            json["search"],
            serde_json::json!({ "name": [{ "start": 0, "end": 2 }], "id": [] })
        );
    }
}
