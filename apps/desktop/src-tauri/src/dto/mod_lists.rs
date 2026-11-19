//! DTOs for `list_working_mods`/`list_inactive_mods`: the Mods page's two
//! working-tier lists. Distinct from [`super::mods::ModSummaryDto`] (the
//! scanned tier's row): two DTOs may not share a Rust type name.

use rim_session::{ModList, ModListQuery, ModListRow, RowState};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::SourceDto;
use super::mods::GeneratedMarkerDto;
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
}

impl TryFrom<ModListQueryDto> for ModListQuery {
    type Error = CommandError;

    fn try_from(value: ModListQueryDto) -> Result<Self, Self::Error> {
        Ok(Self {
            search: value.search,
            tag: value.tag.map(rim_resolve::domain::Tag::new).transpose()?,
            source: value.source.map(Into::into),
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
}

impl From<ModList> for ModListDto {
    fn from(value: ModList) -> Self {
        Self {
            total: value.total,
            items: value.items.iter().map(Into::into).collect(),
        }
    }
}
