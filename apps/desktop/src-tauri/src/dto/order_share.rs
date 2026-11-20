//! DTOs for sharing a load order: `export_order_file`, `export_order_text`,
//! `list_saves`, `preview_order_import_file`, `preview_order_import_text`,
//! `import_order` and `open_workshop_page`.
//!
//! Every outcome is a `kind`-tagged union mirrored from a closed
//! `rim-session` type by an exhaustive `From` (no `_` arm), so a new
//! variant is a compile error here and in TypeScript. Prose never crosses:
//! the frontend words each code. A *name* on an installed row comes from
//! the receiver's own inventory; the sender's name is carried only for a
//! `notInstalled` row (display text, rendered as text).
//!
//! Requests are small structs with `deny_unknown_fields`. None carries a
//! URL; the one path each file command takes comes from the frontend's
//! native file dialog.

use rim_analyzer::domain::ModId;
use rim_session::mod_list::{
    Activation, CorePlacement, ExportedModList, ImportPlan, ImportedEntry, ListAlignment,
    ListFormat, MissingKind, Rejection, SkippedEntry, VersionCheck, render_text,
};
use rim_session::use_cases::{ImportBlocker, ImportOrder, ImportPreview, ReadyImport};
use rim_session::{InventoryEntry, ModInventory};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::patch::ModRefDto;

/// `export_order_file`'s request: where the user's save dialog pointed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportOrderFileRequestDto {
    /// The file to write.
    pub path: String,
}

/// `preview_order_import_file`'s request: the file the user picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PreviewOrderImportFileRequestDto {
    /// The file to read.
    pub path: String,
}

/// `preview_order_import_text`'s request: pasted text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PreviewOrderImportTextRequestDto {
    /// The pasted list.
    pub text: String,
}

/// `import_order`'s request: the `order` a preview returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportOrderRequestDto {
    /// Exact mod ids, in the order to scan with.
    pub order: Vec<String>,
}

/// `open_workshop_page`'s request: a Workshop id as digits. The backend
/// builds the fixed URL; a URL here is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct OpenWorkshopPageRequestDto {
    /// The Workshop item id.
    pub workshop_id: String,
}

/// What an export wrote or rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportResultDto {
    /// How many mods are in the list.
    pub count: usize,
    /// Active ids a mod list cannot hold, left out and reported here.
    pub unrepresentable: Vec<String>,
}

/// `export_order_text`'s answer: the text to copy, and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ExportTextDto {
    /// The list in the text format, for the clipboard.
    pub text: String,
    /// How many mods are in the list.
    pub count: usize,
    /// Active ids a mod list cannot hold, left out and reported here.
    pub unrepresentable: Vec<String>,
}

impl From<&ExportedModList> for ExportResultDto {
    fn from(exported: &ExportedModList) -> Self {
        Self {
            count: exported.list.entries().len(),
            unrepresentable: unrepresentable_ids(exported),
        }
    }
}

impl From<&ExportedModList> for ExportTextDto {
    fn from(exported: &ExportedModList) -> Self {
        Self {
            text: render_text(&exported.list),
            count: exported.list.entries().len(),
            unrepresentable: unrepresentable_ids(exported),
        }
    }
}

fn unrepresentable_ids(exported: &ExportedModList) -> Vec<String> {
    exported
        .unrepresentable
        .iter()
        .map(|id| id.as_str().to_string())
        .collect()
}

/// Why a document cannot be imported. Mirrors [`Rejection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportRejectionDto {
    /// See [`Rejection::TooLarge`].
    #[serde(rename_all = "camelCase")]
    TooLarge {
        /// The size limit in bytes.
        limit_bytes: usize,
    },
    /// See [`Rejection::TooManyEntries`].
    #[serde(rename_all = "camelCase")]
    TooManyEntries {
        /// The entry limit.
        limit: usize,
    },
    /// See [`Rejection::MalformedXml`].
    MalformedXml,
    /// See [`Rejection::DtdNotAllowed`].
    DtdNotAllowed,
    /// See [`Rejection::TooDeep`].
    TooDeep,
    /// See [`Rejection::UnrecognizedFormat`].
    UnrecognizedFormat,
    /// See [`Rejection::MissingModList`].
    MissingModList,
    /// See [`Rejection::NoEntries`].
    NoEntries,
    /// See [`Rejection::SaveHeaderTruncated`].
    SaveHeaderTruncated,
    /// See [`Rejection::SaveHeaderMissing`].
    SaveHeaderMissing,
    /// See [`Rejection::SaveHeaderTooLarge`].
    #[serde(rename_all = "camelCase")]
    SaveHeaderTooLarge {
        /// The header size limit in bytes.
        limit_bytes: usize,
    },
}

impl From<Rejection> for ImportRejectionDto {
    fn from(value: Rejection) -> Self {
        match value {
            Rejection::TooLarge { limit_bytes } => Self::TooLarge { limit_bytes },
            Rejection::TooManyEntries { limit } => Self::TooManyEntries { limit },
            Rejection::MalformedXml => Self::MalformedXml,
            Rejection::DtdNotAllowed => Self::DtdNotAllowed,
            Rejection::TooDeep => Self::TooDeep,
            Rejection::UnrecognizedFormat => Self::UnrecognizedFormat,
            Rejection::MissingModList => Self::MissingModList,
            Rejection::NoEntries => Self::NoEntries,
            Rejection::SaveHeaderTruncated => Self::SaveHeaderTruncated,
            Rejection::SaveHeaderMissing => Self::SaveHeaderMissing,
            Rejection::SaveHeaderTooLarge { limit_bytes } => {
                Self::SaveHeaderTooLarge { limit_bytes }
            }
        }
    }
}

/// Which document shape a list was read from. Mirrors [`ListFormat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ListFormatDto {
    /// See [`ListFormat::SavedModList`].
    SavedModList,
    /// See [`ListFormat::ModsConfig`].
    ModsConfig,
    /// See [`ListFormat::Text`].
    Text,
    /// See [`ListFormat::SaveHeader`].
    SaveHeader,
}

impl From<ListFormat> for ListFormatDto {
    fn from(value: ListFormat) -> Self {
        match value {
            ListFormat::SavedModList => Self::SavedModList,
            ListFormat::ModsConfig => Self::ModsConfig,
            ListFormat::Text => Self::Text,
            ListFormat::SaveHeader => Self::SaveHeader,
        }
    }
}

/// Whether a parallel list (names, Steam ids) lined up with the ids.
/// Mirrors [`ListAlignment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ListAlignmentDto {
    /// See [`ListAlignment::Absent`].
    Absent,
    /// See [`ListAlignment::Aligned`].
    Aligned,
    /// See [`ListAlignment::Misaligned`]: the whole list was left out.
    #[serde(rename_all = "camelCase")]
    Misaligned {
        /// How many ids there were.
        ids: u32,
        /// How many values the parallel list held.
        values: u32,
    },
}

impl From<ListAlignment> for ListAlignmentDto {
    fn from(value: ListAlignment) -> Self {
        match value {
            ListAlignment::Absent => Self::Absent,
            ListAlignment::Aligned => Self::Aligned,
            ListAlignment::Misaligned { ids, values } => Self::Misaligned { ids, values },
        }
    }
}

/// Whether a matched copy is active before the import. Mirrors
/// [`Activation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportActivationDto {
    /// See [`Activation::AlreadyActive`].
    AlreadyActive,
    /// See [`Activation::Activated`].
    Activated,
}

impl From<Activation> for ImportActivationDto {
    fn from(value: Activation) -> Self {
        match value {
            Activation::AlreadyActive => Self::AlreadyActive,
            Activation::Activated => Self::Activated,
        }
    }
}

/// Why a listed mod is missing. Mirrors [`MissingKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MissingKindDto {
    /// See [`MissingKind::Workshop`].
    #[serde(rename_all = "camelCase")]
    Workshop {
        /// The Workshop item id; `#[ts(type = "number")]` as on every DTO.
        #[ts(type = "number")]
        workshop_id: u64,
    },
    /// See [`MissingKind::Dlc`].
    Dlc,
    /// See [`MissingKind::RimmergeMergeMod`].
    RimmergeMergeMod,
    /// See [`MissingKind::NoLink`].
    NoLink,
}

impl From<MissingKind> for MissingKindDto {
    fn from(value: MissingKind) -> Self {
        match value {
            MissingKind::Workshop(id) => Self::Workshop {
                workshop_id: id.get(),
            },
            MissingKind::Dlc => Self::Dlc,
            MissingKind::RimmergeMergeMod => Self::RimmergeMergeMod,
            MissingKind::NoLink => Self::NoLink,
        }
    }
}

/// What the plan does with one listed entry. Mirrors [`ImportedEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportedEntryDto {
    /// See [`ImportedEntry::AlreadyActive`].
    #[serde(rename_all = "camelCase")]
    AlreadyActive {
        /// The exact inventory id.
        id: String,
        /// The receiver's name for it.
        name: String,
    },
    /// See [`ImportedEntry::Activated`].
    #[serde(rename_all = "camelCase")]
    Activated {
        /// The exact inventory id.
        id: String,
        /// The receiver's name for it.
        name: String,
    },
    /// See [`ImportedEntry::MatchedOtherCopy`].
    #[serde(rename_all = "camelCase")]
    MatchedOtherCopy {
        /// The id as listed.
        listed: String,
        /// The exact inventory id of the copy that is used.
        installed: String,
        /// The receiver's name for the copy that is used.
        name: String,
        /// Whether that copy is active now.
        activation: ImportActivationDto,
    },
    /// See [`ImportedEntry::NotInstalled`].
    #[serde(rename_all = "camelCase")]
    NotInstalled {
        /// The id as listed.
        listed: String,
        /// The sender's name for it, when the list carried one. Display
        /// text from an untrusted file: render it as text only.
        name: Option<String>,
        /// What the receiver can do about it.
        missing: MissingKindDto,
    },
    /// See [`ImportedEntry::Duplicate`].
    #[serde(rename_all = "camelCase")]
    Duplicate {
        /// The base id.
        id: String,
        /// 1-based position of the first listing.
        first_position: u32,
    },
}

/// Where Core ends up in the planned order. Mirrors [`CorePlacement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CorePlacementDto {
    /// See [`CorePlacement::Listed`].
    Listed,
    /// See [`CorePlacement::AddedFirst`].
    AddedFirst,
    /// See [`CorePlacement::Missing`].
    Missing,
}

impl From<CorePlacement> for CorePlacementDto {
    fn from(value: CorePlacement) -> Self {
        match value {
            CorePlacement::Listed => Self::Listed,
            CorePlacement::AddedFirst => Self::AddedFirst,
            CorePlacement::Missing => Self::Missing,
        }
    }
}

/// The list's game version against the installed one. Mirrors
/// [`VersionCheck`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum VersionCheckDto {
    /// See [`VersionCheck::Unknown`].
    Unknown,
    /// See [`VersionCheck::Same`].
    Same,
    /// See [`VersionCheck::Differs`].
    #[serde(rename_all = "camelCase")]
    Differs {
        /// The list's version text.
        listed: String,
        /// The installed version text.
        game: String,
    },
}

impl From<VersionCheck> for VersionCheckDto {
    fn from(value: VersionCheck) -> Self {
        match value {
            VersionCheck::Unknown => Self::Unknown,
            VersionCheck::Same => Self::Same,
            VersionCheck::Differs { listed, game } => Self::Differs { listed, game },
        }
    }
}

/// A part of the input that was left out. Mirrors [`SkippedEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum SkippedEntryDto {
    /// See [`SkippedEntry::NotAnEntry`].
    #[serde(rename_all = "camelCase")]
    NotAnEntry {
        /// 1-based line number.
        line: u32,
    },
    /// See [`SkippedEntry::MalformedId`].
    #[serde(rename_all = "camelCase")]
    MalformedId {
        /// 1-based position among the entries of the input.
        position: u32,
        /// A bounded excerpt of the offending text. Untrusted: render it as
        /// text only.
        text: String,
    },
}

impl From<&SkippedEntry> for SkippedEntryDto {
    fn from(value: &SkippedEntry) -> Self {
        match value {
            SkippedEntry::NotAnEntry { line } => Self::NotAnEntry { line: *line },
            SkippedEntry::MalformedId { position, text } => Self::MalformedId {
                position: *position,
                text: text.as_str().to_string(),
            },
        }
    }
}

/// What importing a parsed list would do. Built from an
/// [`ImportPlan`] and the receiver's inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct OrderImportPreviewDto {
    /// How many entries the list had (duplicates included).
    pub listed: usize,
    /// What `import_order` scans with: exact inventory ids, in imported
    /// order. Send it back unchanged.
    pub order: Vec<String>,
    /// One row per listed entry, in list order.
    pub entries: Vec<ImportedEntryDto>,
    /// Active now but not in `order`, in file order. Core and this
    /// profile's own merge mod never appear.
    pub deactivated: Vec<ModRefDto>,
    /// Kept mods whose relative order differs from the file's.
    pub moved: u32,
    /// Where Core ends up.
    pub core: CorePlacementDto,
    /// How the list's game version compares with the installed one.
    pub version: VersionCheckDto,
    /// The document shape the list was read from.
    pub format: ListFormatDto,
    /// The game version text the list carries, verbatim (untrusted: render
    /// it as text only), or `null` when it has none.
    pub listed_game_version: Option<String>,
    /// Whether the list's names lined up with its ids; a misaligned list
    /// was left out whole.
    pub names: ListAlignmentDto,
    /// The same for the Steam ids behind the Workshop links.
    pub workshop_ids: ListAlignmentDto,
    /// Whether the import replaces activation edits not yet rescanned.
    pub replaces_pending_changes: bool,
    /// What the parser left out, bounded.
    pub skipped: Vec<SkippedEntryDto>,
    /// How many further skipped parts were not listed in `skipped`.
    pub omitted_skipped: usize,
    /// Why `import_order` would refuse `order` ([`ImportOrder::import_blocker`]),
    /// or `null` when it would accept it; any reason disables "Use this order".
    pub import_blocked: Option<ImportBlockedDto>,
}

/// Why an order cannot be imported. Mirrors [`ImportBlocker`]; the scan
/// check (`ImportOrderError::ScanDidNotMatch`) happens after the scan and
/// cannot arise from a preview, so it has no variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ImportBlockedDto {
    /// See [`ImportBlocker::CoreMissing`].
    CoreMissing,
    /// See [`ImportBlocker::NothingInstalled`].
    NothingInstalled,
    /// See [`ImportBlocker::TooMany`].
    #[serde(rename_all = "camelCase")]
    TooMany {
        /// The most mods an imported order may hold.
        limit: usize,
    },
    /// See [`ImportBlocker::Unknown`]: an id that names no installed mod
    /// (also one known but not on disk).
    #[serde(rename_all = "camelCase")]
    Unknown {
        /// The offending id.
        id: String,
    },
    /// See [`ImportBlocker::Duplicate`].
    #[serde(rename_all = "camelCase")]
    Duplicate {
        /// The repeated id.
        id: String,
    },
}

impl From<ImportBlocker> for ImportBlockedDto {
    fn from(value: ImportBlocker) -> Self {
        match value {
            ImportBlocker::CoreMissing => Self::CoreMissing,
            ImportBlocker::NothingInstalled => Self::NothingInstalled,
            ImportBlocker::TooMany { limit } => Self::TooMany { limit },
            ImportBlocker::Unknown(id) => Self::Unknown {
                id: id.as_str().to_string(),
            },
            ImportBlocker::Duplicate(id) => Self::Duplicate {
                id: id.as_str().to_string(),
            },
        }
    }
}

/// The outcome of previewing an import: a document that cannot be read as
/// a list is an outcome the frontend words, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
#[expect(
    clippy::large_enum_variant,
    reason = "one value per preview, serialized at once and dropped: boxing the ready half would only add an indirection to every reader"
)]
pub enum OrderImportOutcomeDto {
    /// See [`ImportPreview::Rejected`].
    Rejected {
        /// Why the document cannot be imported.
        reason: ImportRejectionDto,
    },
    /// See [`ImportPreview::Ready`].
    Ready {
        /// The diff against this install.
        preview: OrderImportPreviewDto,
    },
}

impl OrderImportOutcomeDto {
    /// Maps a preview. `inventory` is the receiver's own, the source of
    /// every installed row's name.
    #[must_use]
    pub fn from_preview(preview: &ImportPreview, inventory: &ModInventory) -> Self {
        match preview {
            ImportPreview::Rejected(rejection) => Self::Rejected {
                reason: (*rejection).into(),
            },
            ImportPreview::Ready(ready) => Self::Ready {
                preview: preview_dto(ready, inventory),
            },
        }
    }
}

fn preview_dto(ready: &ReadyImport, inventory: &ModInventory) -> OrderImportPreviewDto {
    let plan: &ImportPlan = &ready.plan;
    OrderImportPreviewDto {
        listed: plan.entries.len(),
        order: plan
            .order
            .iter()
            .map(|id| id.as_str().to_string())
            .collect(),
        entries: plan
            .entries
            .iter()
            .map(|entry| entry_dto(entry, inventory))
            .collect(),
        deactivated: plan
            .deactivated
            .iter()
            .map(|id| ModRefDto {
                mod_id: id.as_str().to_string(),
                name: installed_name(id, inventory),
            })
            .collect(),
        moved: plan.moved,
        core: plan.core.into(),
        version: plan.version.clone().into(),
        format: ready.format.into(),
        listed_game_version: ready
            .listed_game_version
            .as_ref()
            .map(|version| version.as_str().to_string()),
        names: ready.names.into(),
        workshop_ids: ready.workshop_ids.into(),
        replaces_pending_changes: plan.replaces_pending_changes,
        skipped: ready.skipped.iter().map(Into::into).collect(),
        omitted_skipped: ready.omitted_skipped,
        import_blocked: ImportOrder::import_blocker(inventory, plan).map(Into::into),
    }
}

/// The receiver's name for an id. Ids in the order or the entries of an
/// import came out of the inventory (the planner resolves through it); the
/// deactivated ids come from the file's list instead, and a missing mod
/// (listed in `ModsConfig.xml`, not on disk) sits in the inventory with
/// `name = id`. So the id itself as a fallback is display text for an id the
/// inventory does not know at all, not a masked failure.
fn installed_name(id: &ModId, inventory: &ModInventory) -> String {
    inventory
        .entry(id)
        .map(|entry: &InventoryEntry| entry.name.clone())
        .unwrap_or_else(|| id.as_str().to_string())
}

fn entry_dto(entry: &ImportedEntry, inventory: &ModInventory) -> ImportedEntryDto {
    match entry {
        ImportedEntry::AlreadyActive { id } => ImportedEntryDto::AlreadyActive {
            id: id.as_str().to_string(),
            name: installed_name(id, inventory),
        },
        ImportedEntry::Activated { id } => ImportedEntryDto::Activated {
            id: id.as_str().to_string(),
            name: installed_name(id, inventory),
        },
        ImportedEntry::MatchedOtherCopy {
            listed,
            installed,
            activation,
        } => ImportedEntryDto::MatchedOtherCopy {
            listed: listed.as_str().to_string(),
            installed: installed.as_str().to_string(),
            name: installed_name(installed, inventory),
            activation: (*activation).into(),
        },
        ImportedEntry::NotInstalled { listed, name, kind } => ImportedEntryDto::NotInstalled {
            listed: listed.as_str().to_string(),
            name: name.as_ref().map(|name| name.as_str().to_string()),
            missing: (*kind).into(),
        },
        ImportedEntry::Duplicate { id, first_position } => ImportedEntryDto::Duplicate {
            id: id.as_str().to_string(),
            first_position: *first_position,
        },
    }
}

/// One save in RimWorld's `Saves` folder, for the "Import from a save"
/// list. A save's name is the player's own text: render it as text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SaveFileDto {
    /// The file's full path, sent back unchanged to `preview_order_import_file`.
    pub path: String,
    /// The file name without its extension.
    pub name: String,
    /// When the file was last modified, in seconds since the Unix epoch;
    /// `null` when the file system gave no plausible time.
    #[ts(type = "number | null")]
    pub saved_at: Option<u64>,
}

/// `list_saves`' answer: the saves, newest first, at most 200.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SaveListingDto {
    /// The `Saves` folder that was listed, or `null` when RimWorld's
    /// `Saves` folder was not found beside the `Config` folder.
    pub folder: Option<String>,
    /// The saves, newest first.
    pub saves: Vec<SaveFileDto>,
    /// How many further saves exist but are not listed.
    #[ts(type = "number")]
    pub omitted: u64,
}

impl SaveListingDto {
    /// The answer when there is no `Saves` folder to list.
    pub(crate) fn no_folder() -> Self {
        Self {
            folder: None,
            saves: Vec::new(),
            omitted: 0,
        }
    }

    /// Maps a listing of `folder`.
    pub(crate) fn from_listing(folder: &std::path::Path, listing: rim_io::SaveListing) -> Self {
        Self {
            folder: Some(folder.display().to_string()),
            saves: listing
                .saves
                .into_iter()
                .map(|save| SaveFileDto {
                    path: save.path.display().to_string(),
                    name: save.name,
                    saved_at: save.saved_at,
                })
                .collect(),
            // Saturating, as the other count conversions here: the examination bound makes
            // overflow unreachable, but a wrong count must never wrap silently.
            omitted: u64::try_from(listing.omitted).unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_listing_ships_camel_case_fields_and_a_numeric_time() {
        let listing = SaveListingDto {
            folder: Some("Saves".to_string()),
            saves: vec![
                SaveFileDto {
                    path: "Saves/A.rws".to_string(),
                    name: "A".to_string(),
                    saved_at: Some(1_700_000_000),
                },
                SaveFileDto {
                    path: "Saves/B.rws".to_string(),
                    name: "B".to_string(),
                    saved_at: None,
                },
            ],
            omitted: 2,
        };

        let value = serde_json::to_value(&listing).expect("serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "folder": "Saves",
                "saves": [
                    {"path": "Saves/A.rws", "name": "A", "savedAt": 1_700_000_000_u64},
                    {"path": "Saves/B.rws", "name": "B", "savedAt": null},
                ],
                "omitted": 2,
            })
        );
    }

    #[test]
    fn every_request_rejects_a_field_it_does_not_define() {
        assert!(
            serde_json::from_str::<ExportOrderFileRequestDto>(
                r#"{"path":"a.rml","overwrite":true}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PreviewOrderImportFileRequestDto>(
                r#"{"path":"a.rml","url":"https://example.invalid"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PreviewOrderImportTextRequestDto>(
                r#"{"text":"example.mod","path":"a.rml"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ImportOrderRequestDto>(r#"{"order":[],"force":true}"#).is_err()
        );
        assert!(
            serde_json::from_str::<OpenWorkshopPageRequestDto>(
                r#"{"workshopId":"1","url":"https://example.invalid"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn requests_read_camel_case_fields() {
        let request: OpenWorkshopPageRequestDto =
            serde_json::from_str(r#"{"workshopId":"1234567890"}"#).expect("parses");

        assert_eq!(request.workshop_id, "1234567890");
    }

    #[test]
    fn a_rejection_with_a_payload_ships_its_fields_in_camel_case() {
        let json = serde_json::to_value(ImportRejectionDto::from(Rejection::TooLarge {
            limit_bytes: 4,
        }))
        .expect("serializes");

        assert_eq!(
            json,
            serde_json::json!({"kind": "tooLarge", "limitBytes": 4})
        );
    }

    #[test]
    fn the_save_rejections_ship_their_kind_and_camel_case_fields() {
        let truncated =
            serde_json::to_value(ImportRejectionDto::from(Rejection::SaveHeaderTruncated))
                .expect("serializes");
        let missing = serde_json::to_value(ImportRejectionDto::from(Rejection::SaveHeaderMissing))
            .expect("serializes");
        let too_large =
            serde_json::to_value(ImportRejectionDto::from(Rejection::SaveHeaderTooLarge {
                limit_bytes: 4,
            }))
            .expect("serializes");

        assert_eq!(
            truncated,
            serde_json::json!({"kind": "saveHeaderTruncated"})
        );
        assert_eq!(missing, serde_json::json!({"kind": "saveHeaderMissing"}));
        assert_eq!(
            too_large,
            serde_json::json!({"kind": "saveHeaderTooLarge", "limitBytes": 4})
        );
    }

    #[test]
    fn a_list_format_and_an_alignment_ship_as_a_name_and_a_tagged_union() {
        let format =
            serde_json::to_value(ListFormatDto::from(ListFormat::SaveHeader)).expect("serializes");
        let aligned = serde_json::to_value(ListAlignmentDto::from(ListAlignment::Aligned))
            .expect("serializes");
        let misaligned = serde_json::to_value(ListAlignmentDto::from(ListAlignment::Misaligned {
            ids: 4,
            values: 3,
        }))
        .expect("serializes");

        assert_eq!(format, serde_json::json!("saveHeader"));
        assert_eq!(aligned, serde_json::json!({"kind": "aligned"}));
        assert_eq!(
            misaligned,
            serde_json::json!({"kind": "misaligned", "ids": 4, "values": 3})
        );
    }

    #[test]
    fn a_duplicate_row_and_a_workshop_kind_ship_camel_case_fields() {
        let duplicate = serde_json::to_value(ImportedEntryDto::Duplicate {
            id: "example.framework".to_string(),
            first_position: 2,
        })
        .expect("serializes");
        let workshop = serde_json::to_value(MissingKindDto::Workshop {
            workshop_id: 1_234_567_890,
        })
        .expect("serializes");

        assert_eq!(
            duplicate,
            serde_json::json!({"kind": "duplicate", "id": "example.framework", "firstPosition": 2})
        );
        assert_eq!(
            workshop,
            serde_json::json!({"kind": "workshop", "workshopId": 1_234_567_890_u64})
        );
    }

    #[test]
    fn an_import_blocker_ships_its_kind_and_camel_case_fields() {
        let too_many = serde_json::to_value(ImportBlockedDto::from(ImportBlocker::TooMany {
            limit: 5_002,
        }))
        .expect("serializes");
        let unknown = serde_json::to_value(ImportBlockedDto::from(ImportBlocker::Unknown(
            ModId::new("example.gone"),
        )))
        .expect("serializes");
        let nothing = serde_json::to_value(ImportBlockedDto::from(ImportBlocker::NothingInstalled))
            .expect("serializes");

        assert_eq!(
            too_many,
            serde_json::json!({"kind": "tooMany", "limit": 5_002})
        );
        assert_eq!(
            unknown,
            serde_json::json!({"kind": "unknown", "id": "example.gone"})
        );
        assert_eq!(nothing, serde_json::json!({"kind": "nothingInstalled"}));
    }
}
