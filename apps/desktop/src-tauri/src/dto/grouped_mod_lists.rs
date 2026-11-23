//! DTOs for `list_working_mods_grouped`/`list_inactive_mods_grouped`: the
//! Mods page's Groups view. A structural superset of
//! [`super::mod_lists::ModListDto`] (`total`, `items`, `reorderable`) plus
//! the flat folder/mod entries, so a consumer of `items` does not fork by
//! view. Every type name here is new: two DTOs may not share a Rust type
//! name.

use rim_session::{
    Folder, FolderKey, GroupedEntry, GroupedListQuery, GroupedModList, MAX_COLLAPSED_FOLDERS,
    TooManyCollapsedFolders,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::mod_lists::{ModListQueryDto, ModListRowDto};
use super::organization::{ColorTokenDto, parse_group_id};
use crate::error::CommandError;

/// Which folder of the Groups view a fold or an entry refers to. Mirrors
/// [`FolderKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FolderKeyDto {
    /// A group's folder.
    #[serde(rename_all = "camelCase")]
    Group {
        /// The group.
        group_id: u32,
    },
    /// The folder of mods that are in no group.
    Ungrouped,
}

impl TryFrom<FolderKeyDto> for FolderKey {
    type Error = CommandError;

    fn try_from(value: FolderKeyDto) -> Result<Self, CommandError> {
        match value {
            FolderKeyDto::Group { group_id } => Ok(Self::Group(parse_group_id(group_id)?)),
            FolderKeyDto::Ungrouped => Ok(Self::Ungrouped),
        }
    }
}

impl From<FolderKey> for FolderKeyDto {
    fn from(value: FolderKey) -> Self {
        match value {
            FolderKey::Group(id) => Self::Group { group_id: id.get() },
            FolderKey::Ungrouped => Self::Ungrouped,
        }
    }
}

/// Request shape for the two grouped list commands: the pane's own query
/// and the folders folded shut.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupedListQueryDto {
    /// The pane's own filter and sort, unchanged.
    pub list: ModListQueryDto,
    /// The folded folders. A key naming no folder is ignored; more than
    /// one per group plus one is refused.
    pub collapsed: Vec<FolderKeyDto>,
}

impl From<TooManyCollapsedFolders> for CommandError {
    fn from(error: TooManyCollapsedFolders) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl TryFrom<GroupedListQueryDto> for GroupedListQuery {
    type Error = CommandError;

    fn try_from(value: GroupedListQueryDto) -> Result<Self, CommandError> {
        // Bound the raw array before mapping it: duplicates collapse in the
        // set, so the domain's distinct-key check alone would let a huge
        // repeated list be walked first.
        if value.collapsed.len() > MAX_COLLAPSED_FOLDERS {
            return Err(TooManyCollapsedFolders {
                count: value.collapsed.len(),
            }
            .into());
        }
        let collapsed = value
            .collapsed
            .into_iter()
            .map(FolderKey::try_from)
            .collect::<Result<_, _>>()?;
        Ok(Self::new(value.list.try_into()?, collapsed)?)
    }
}

/// A folder header. Mirrors [`Folder`]: "Ungrouped" has no name or
/// colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FolderDto {
    /// A group.
    #[serde(rename_all = "camelCase")]
    Group {
        /// The group's id.
        id: u32,
        /// Its name.
        name: String,
        /// Its colour.
        color: Option<ColorTokenDto>,
    },
    /// The mods that are in no group.
    Ungrouped,
}

impl From<&Folder> for FolderDto {
    fn from(value: &Folder) -> Self {
        match value {
            Folder::Group { id, name, color } => Self::Group {
                id: id.get(),
                name: name.to_string(),
                color: color.map(Into::into),
            },
            Folder::Ungrouped => Self::Ungrouped,
        }
    }
}

/// One line of the Groups view. Mirrors [`GroupedEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GroupedEntryDto {
    /// A folder header.
    #[serde(rename_all = "camelCase")]
    Folder {
        /// Which folder.
        folder: FolderDto,
        /// 1 is the top level of this view (rebased under a group scope).
        depth: usize,
        /// Distinct mods in the folder and every subfolder, in this pane,
        /// admitted by the pane's whole query. Folding never changes it.
        matched: usize,
        /// Whether the folder is folded shut.
        is_collapsed: bool,
    },
    /// One copy of a mod inside a folder.
    #[serde(rename_all = "camelCase")]
    Mod {
        /// Joins to `items`.
        mod_id: String,
        /// The folder this copy sits in.
        folder: FolderKeyDto,
        /// The folder's depth plus one.
        depth: usize,
        /// The mod's other direct groups.
        also_in: usize,
    },
}

impl From<&GroupedEntry> for GroupedEntryDto {
    fn from(value: &GroupedEntry) -> Self {
        match value {
            GroupedEntry::Folder {
                folder,
                depth,
                matched,
                is_collapsed,
            } => Self::Folder {
                folder: folder.into(),
                depth: *depth,
                matched: *matched,
                is_collapsed: *is_collapsed,
            },
            GroupedEntry::Mod {
                mod_id,
                folder,
                depth,
                also_in,
            } => Self::Mod {
                mod_id: mod_id.as_str().to_string(),
                folder: (*folder).into(),
                depth: *depth,
                also_in: *also_in,
            },
        }
    }
}

/// One pane in Groups view. `total` rows matched; `items` holds at most the
/// backend's own bound; `entries` lays them out as folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GroupedModListDto {
    /// Rows matching the query before the bound.
    pub total: usize,
    /// The rows, once each, in list order.
    pub items: Vec<ModListRowDto>,
    /// Always `false` in this view: a row's place on screen is not its place
    /// in the load order.
    pub reorderable: bool,
    /// The visible entries, in order.
    pub entries: Vec<GroupedEntryDto>,
    /// Entries before the backend's bound, so a cut shows.
    pub total_entries: usize,
}

impl From<GroupedModList> for GroupedModListDto {
    fn from(value: GroupedModList) -> Self {
        Self {
            total: value.list.total,
            items: value.list.items.iter().map(Into::into).collect(),
            reorderable: value.list.reorderable,
            entries: value.entries.iter().map(Into::into).collect(),
            total_entries: value.total_entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_request_is_camel_case_with_kind_tagged_folder_keys_and_refuses_unknown_fields() {
        let request: GroupedListQueryDto = serde_json::from_value(json!({
            "list": { "search": null, "tag": null, "source": null, "sort": null },
            "collapsed": [{ "kind": "group", "groupId": 3 }, { "kind": "ungrouped" }],
        }))
        .expect("the wire shape parses");
        let unknown = serde_json::from_value::<GroupedListQueryDto>(json!({
            "list": {}, "collapsed": [], "extra": 1,
        }));

        let query = GroupedListQuery::try_from(request).expect("valid keys");

        assert_eq!(query.collapsed().len(), 2);
        assert!(unknown.is_err());
    }

    fn request_with(collapsed: Vec<FolderKeyDto>) -> GroupedListQueryDto {
        GroupedListQueryDto {
            list: ModListQueryDto::default(),
            collapsed,
        }
    }

    #[test]
    fn a_raw_collapsed_array_past_the_bound_is_refused_even_when_its_keys_repeat() {
        let at_the_bound = request_with(vec![FolderKeyDto::Ungrouped; MAX_COLLAPSED_FOLDERS]);
        let past_the_bound = request_with(vec![FolderKeyDto::Ungrouped; MAX_COLLAPSED_FOLDERS + 1]);

        let accepted = GroupedListQuery::try_from(at_the_bound).expect("within the bound");
        let refused = GroupedListQuery::try_from(past_the_bound).expect_err("one key too many");

        assert_eq!(accepted.collapsed().len(), 1);
        assert_eq!(refused.code, crate::error::CommandErrorCode::InvalidInput);
    }

    #[test]
    fn entries_serialize_with_kind_tags_and_camel_case_fields() {
        let folder = GroupedEntryDto::Folder {
            folder: FolderDto::Group {
                id: 2,
                name: "Weapons".to_string(),
                color: None,
            },
            depth: 1,
            matched: 4,
            is_collapsed: true,
        };
        let copy = GroupedEntryDto::Mod {
            mod_id: "a".to_string(),
            folder: FolderKeyDto::Ungrouped,
            depth: 2,
            also_in: 0,
        };

        assert_eq!(
            serde_json::to_value(&folder).expect("serializes"),
            json!({
                "kind": "folder",
                "folder": { "kind": "group", "id": 2, "name": "Weapons", "color": null },
                "depth": 1,
                "matched": 4,
                "isCollapsed": true,
            })
        );
        assert_eq!(
            serde_json::to_value(&copy).expect("serializes"),
            json!({
                "kind": "mod",
                "modId": "a",
                "folder": { "kind": "ungrouped" },
                "depth": 2,
                "alsoIn": 0,
            })
        );
    }
}
