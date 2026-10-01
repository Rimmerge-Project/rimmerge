//! DTOs for `get_mod_info`/`read_mod_preview`: the mod info panel's
//! backend half.

use std::path::Path;

use rim_analyzer::extract::rich_text::RichRun;
use rim_session::mod_info::{
    ActiveModInfo, HomepageLink, InactiveModInfo, MissingModInfo, ModInfo, PendingChange,
};
use rim_session::use_cases::{
    AboutOutcome, AboutUnreadable, ModInfoWithAbout, ModPreview, PreviewUnreadable,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{SourceDto, TierDto};
use super::mods::{DeclaredOrderDto, GeneratedMarkerDto};
use super::startup::{ModCostRowDto, mod_cost_row};
use super::texture::{TextureFormatDto, encode_data_url};

/// Mirrors [`PendingChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PendingChangeDto {
    /// See [`PendingChange::ActivationPending`].
    ActivationPending,
    /// See [`PendingChange::DeactivationPending`].
    DeactivationPending,
}

impl From<PendingChange> for PendingChangeDto {
    fn from(value: PendingChange) -> Self {
        match value {
            PendingChange::ActivationPending => Self::ActivationPending,
            PendingChange::DeactivationPending => Self::DeactivationPending,
        }
    }
}

/// Mirrors [`HomepageLink`]. `Openable`'s own `url` is display-only — a
/// click always calls `open_mod_link(modId, "homepage")`, never sends
/// this text back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum HomepageLinkDto {
    /// See [`HomepageLink::Openable`].
    Openable {
        /// The URL text, for display only.
        url: String,
    },
    /// See [`HomepageLink::Text`].
    Text {
        /// The raw, non-openable value.
        text: String,
    },
}

impl From<&HomepageLink> for HomepageLinkDto {
    fn from(value: &HomepageLink) -> Self {
        match value {
            HomepageLink::Openable(url) => Self::Openable {
                url: url.as_str().to_string(),
            },
            HomepageLink::Text(text) => Self::Text { text: text.clone() },
        }
    }
}

/// One sanitized description run. Mirrors [`RichRun`]/`RunStyle` —
/// **never** rendered with `v-html` on the frontend; `text` reaches the
/// DOM only through ordinary interpolation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RichRunDto {
    /// The run's plain text — BBCode and any unmatched/unrecognized tag
    /// stay literal here, exactly as typed.
    pub text: String,
    /// Whether this run is bold.
    pub bold: bool,
    /// Whether this run is italic.
    pub italic: bool,
}

impl From<&RichRun> for RichRunDto {
    fn from(value: &RichRun) -> Self {
        Self {
            text: value.text.clone(),
            bold: value.style.bold,
            italic: value.style.italic,
        }
    }
}

/// A sanitized description: its runs, plus whether the source text was
/// truncated before parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DescriptionDto {
    /// The sanitized runs, in order.
    pub runs: Vec<RichRunDto>,
    /// Whether the source text was truncated before parsing (over 32
    /// KiB).
    pub truncated: bool,
}

/// Mirrors [`AboutOutcome`] — the lazily-read `About.xml` half of the mod
/// info panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModAboutDto {
    /// Read and parsed successfully.
    Read {
        /// `<modVersion>`, verbatim.
        ///
        /// `#[serde(rename = "modVersion")]`: `#[serde(tag = "kind",
        /// rename_all = "camelCase")]` on the enum only renames
        /// *variant* names, not a struct variant's own fields — see
        /// `apps/desktop/CLAUDE.md`'s own trap entry for
        /// `CommandErrorDetail`.
        #[serde(rename = "modVersion")]
        mod_version: Option<String>,
        /// `<modIconPath>`, verbatim.
        #[serde(rename = "modIconPath")]
        mod_icon_path: Option<String>,
        /// `None` when `<description>` is absent — distinct from an
        /// empty run list, which never happens (an empty description
        /// parses to `None` upstream too).
        description: Option<DescriptionDto>,
        /// The re-read `<url>`, classified for the "open in browser"
        /// affordance — the only homepage source for an **inactive**
        /// mod; an active mod's own top-level `homepage` field (scan-time)
        /// is preferred for that variant.
        homepage: Option<HomepageLinkDto>,
    },
    /// See [`AboutOutcome::Changed`].
    Changed,
    /// See [`AboutOutcome::Unreadable`].
    Unreadable {
        /// Why the file couldn't be shown; the frontend renders the
        /// sentence.
        cause: AboutUnreadableCauseDto,
        /// The underlying error's English text — a technical-details
        /// line only.
        detail: String,
    },
    /// See [`AboutOutcome::NotOnDisk`].
    NotOnDisk,
}

impl From<&AboutOutcome> for ModAboutDto {
    fn from(value: &AboutOutcome) -> Self {
        match value {
            AboutOutcome::Read {
                mod_version,
                mod_icon_path,
                description,
                homepage,
            } => Self::Read {
                mod_version: mod_version.clone(),
                mod_icon_path: mod_icon_path.clone(),
                description: description.as_ref().map(|text| DescriptionDto {
                    runs: text.runs.iter().map(Into::into).collect(),
                    truncated: text.truncated,
                }),
                homepage: homepage.as_ref().map(Into::into),
            },
            AboutOutcome::Changed => Self::Changed,
            AboutOutcome::Unreadable(AboutUnreadable::Io { detail }) => Self::Unreadable {
                cause: AboutUnreadableCauseDto::Io,
                detail: detail.clone(),
            },
            AboutOutcome::Unreadable(AboutUnreadable::Xml { detail }) => Self::Unreadable {
                cause: AboutUnreadableCauseDto::Xml,
                detail: detail.clone(),
            },
            AboutOutcome::NotOnDisk => Self::NotOnDisk,
        }
    }
}

/// Why an `About.xml` that exists couldn't be shown. Mirrors
/// [`AboutUnreadable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum AboutUnreadableCauseDto {
    /// See [`AboutUnreadable::Io`].
    Io,
    /// See [`AboutUnreadable::Xml`].
    Xml,
}

/// [`ModInfoDto::Active`]'s own payload, boxed on the enum (see
/// [`ModInfoDto`]'s own doc comment for why) — mirrors
/// [`rim_session::mod_info::ActiveModInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ActiveModInfoDto {
    /// The mod's id.
    pub mod_id: String,
    /// Its display name.
    pub name: String,
    /// Its listed authors.
    pub authors: Vec<String>,
    /// Its `About.xml` `url`, classified for the "open in browser"
    /// affordance.
    pub homepage: Option<HomepageLinkDto>,
    /// Where its files come from.
    pub source: SourceDto,
    /// Its `supportedVersions` list.
    pub supported_versions: Vec<String>,
    /// Whether `supported_versions` lists the report's own scanned game
    /// version.
    pub supports_game_version: bool,
    /// Its declared load-order hints.
    pub declared: DeclaredOrderDto,
    /// This mod's own root folder, shown relative to the game directory
    /// or the workshop content folder (never an absolute path, which
    /// would leak the user's own home directory into a screenshot).
    pub root: String,
    /// Folders this mod loads from, relative to `root`, in
    /// load-priority order.
    pub loaded_folders: Vec<String>,
    /// This mod's own scan-cost row, when the report carries one.
    pub cost: Option<ModCostRowDto>,
    /// Every tag it currently carries.
    pub tags: Vec<String>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    pub hard_dependents: usize,
    /// Distinct active mods with a Soft-strength edge pointing at it.
    pub soft_dependents: usize,
    /// Distinct active mods with an Awareness-strength edge pointing at
    /// it.
    pub awareness_dependents: usize,
    /// Whether it looks like a shared framework other mods build on.
    pub is_framework_candidate: bool,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarkerDto>,
    /// Its Steam Workshop published-file id.
    #[ts(type = "number | null")]
    pub workshop_id: Option<u64>,
    /// Zero-based position in the selected order.
    pub position: usize,
    /// Position in the current order, when the selected order is the
    /// suggested one.
    pub previous_position: Option<usize>,
    /// The tier the sorter assigned it.
    pub tier: TierDto,
    /// Live findings naming this mod, in the selected order.
    pub findings_total: usize,
    /// Of those, how many need user input.
    pub needs_input_count: usize,
    /// Whether an uncommitted activate/deactivate edit is pending on it.
    pub pending: Option<PendingChangeDto>,
    /// The lazily-read `About.xml` details.
    pub about: ModAboutDto,
}

/// [`ModInfoDto::Inactive`]'s own payload, boxed on the enum — mirrors
/// [`rim_session::mod_info::InactiveModInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct InactiveModInfoDto {
    /// The mod's id.
    pub mod_id: String,
    /// Its display name.
    pub name: String,
    /// Its listed authors.
    pub authors: Vec<String>,
    /// Where its files come from.
    pub source: SourceDto,
    /// Its `supportedVersions` list.
    pub supported_versions: Vec<String>,
    /// Its declared load-order hints.
    pub declared: DeclaredOrderDto,
    /// See [`ActiveModInfoDto::root`].
    pub root: String,
    /// Its Steam Workshop published-file id.
    #[ts(type = "number | null")]
    pub workshop_id: Option<u64>,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarkerDto>,
    /// Whether an uncommitted activate/deactivate edit is pending on it.
    pub pending: Option<PendingChangeDto>,
    /// The lazily-read `About.xml` details.
    pub about: ModAboutDto,
}

/// [`ModInfoDto::Missing`]'s own payload — mirrors
/// [`rim_session::mod_info::MissingModInfo`]. Not boxed: it's already
/// small (an id and a list), and boxing only the two large variants is
/// enough to close the size gap `clippy::large_enum_variant` flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MissingModInfoDto {
    /// The mod's id.
    pub mod_id: String,
    /// Every active mod that declares a dependency on this id.
    pub required_by: Vec<String>,
}

/// `get_mod_info`'s response. Mirrors [`ModInfo`] plus the lazily-read
/// `about` half — `Missing` carries no `about` at all (there's no
/// `About.xml` to have read). `Active`/`Inactive` are boxed, mirroring
/// [`ModInfo`]'s own boxing, for the identical reason: both carry enough
/// fields that the smallest variant would otherwise pay for the largest
/// one's stack size on every value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModInfoDto {
    /// See [`ModInfo::Active`].
    Active(Box<ActiveModInfoDto>),
    /// See [`ModInfo::Inactive`].
    Inactive(Box<InactiveModInfoDto>),
    /// See [`ModInfo::Missing`].
    Missing(MissingModInfoDto),
}

/// Shows `path` relative to `game_dir` or `workshop_dir`, whichever it's
/// actually under — falls back to the absolute path only when it's under
/// neither (shouldn't happen for a real mod root, but never panics over
/// it).
fn display_relative(path: &Path, game_dir: &Path, workshop_dir: &Path) -> String {
    for base in [game_dir, workshop_dir] {
        if let Ok(relative) = path.strip_prefix(base) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}

fn active_mod_info_dto(
    value: &ActiveModInfo,
    about: ModAboutDto,
    game_dir: &Path,
    workshop_dir: &Path,
) -> ModInfoDto {
    let root_display = display_relative(&value.root, game_dir, workshop_dir);
    ModInfoDto::Active(Box::new(ActiveModInfoDto {
        mod_id: value.mod_id.as_str().to_string(),
        name: value.name.clone(),
        authors: value.authors.clone(),
        homepage: value.homepage.as_ref().map(Into::into),
        source: value.source.into(),
        supported_versions: value.supported_versions.clone(),
        supports_game_version: value.supports_game_version,
        declared: (&value.declared).into(),
        root: root_display,
        loaded_folders: value
            .loaded_folders
            .iter()
            .map(|folder| {
                folder.strip_prefix(&value.root).map_or_else(
                    |_| folder.display().to_string(),
                    |rel| rel.display().to_string(),
                )
            })
            .collect(),
        cost: value.cost.as_ref().map(mod_cost_row),
        tags: value.tags.iter().map(ToString::to_string).collect(),
        hard_dependents: value.hard_dependents,
        soft_dependents: value.soft_dependents,
        awareness_dependents: value.awareness_dependents,
        is_framework_candidate: value.is_framework_candidate,
        generated: value.generated.as_ref().map(Into::into),
        workshop_id: value.workshop_id,
        position: value.position,
        previous_position: value.previous_position,
        tier: value.tier.into(),
        findings_total: value.findings_total,
        needs_input_count: value.needs_input_count,
        pending: value.pending.map(Into::into),
        about,
    }))
}

fn inactive_mod_info_dto(
    value: &InactiveModInfo,
    about: ModAboutDto,
    game_dir: &Path,
    workshop_dir: &Path,
) -> ModInfoDto {
    ModInfoDto::Inactive(Box::new(InactiveModInfoDto {
        mod_id: value.mod_id.as_str().to_string(),
        name: value.name.clone(),
        authors: value.authors.clone(),
        source: value.source.into(),
        supported_versions: value.supported_versions.clone(),
        declared: (&value.declared).into(),
        root: display_relative(&value.root, game_dir, workshop_dir),
        workshop_id: value.workshop_id,
        generated: value.generated.as_ref().map(Into::into),
        pending: value.pending.map(Into::into),
        about,
    }))
}

fn missing_mod_info_dto(value: &MissingModInfo) -> ModInfoDto {
    ModInfoDto::Missing(MissingModInfoDto {
        mod_id: value.mod_id.as_str().to_string(),
        required_by: value
            .required_by
            .iter()
            .map(|id| id.as_str().to_string())
            .collect(),
    })
}

/// Builds `get_mod_info`'s response from the resolved [`ModInfoWithAbout`],
/// showing [`ActiveModInfo::root`]/[`InactiveModInfo::root`] relative to
/// `game_dir`/`workshop_dir`.
#[must_use]
pub fn mod_info_dto(value: &ModInfoWithAbout, game_dir: &Path, workshop_dir: &Path) -> ModInfoDto {
    let about: ModAboutDto = (&value.about).into();
    match &value.info {
        ModInfo::Active(active) => active_mod_info_dto(active, about, game_dir, workshop_dir),
        ModInfo::Inactive(inactive) => {
            inactive_mod_info_dto(inactive, about, game_dir, workshop_dir)
        }
        ModInfo::Missing(missing) => missing_mod_info_dto(missing),
    }
}

/// Why a located preview file couldn't be read back. Mirrors
/// [`PreviewUnreadable`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PreviewUnreadableReasonDto {
    /// See [`PreviewUnreadable::TooLarge`].
    TooLarge,
    /// See [`PreviewUnreadable::UnsupportedFormat`].
    UnsupportedFormat,
    /// See [`PreviewUnreadable::Io`].
    Io,
}

/// `read_mod_preview`'s response. Mirrors [`ModPreview`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModPreviewDto {
    /// `About/Preview.png` was read successfully.
    Image {
        /// A `data:image/<format>;base64,...` URL, ready for an `<img
        /// src>`.
        #[serde(rename = "dataUrl")]
        data_url: String,
        /// The sniffed image format.
        format: TextureFormatDto,
        /// The file's raw byte count (before base64 encoding).
        bytes: usize,
    },
    /// See [`ModPreview::Absent`].
    Absent,
    /// See [`ModPreview::NotOnDisk`].
    NotOnDisk,
    /// See [`ModPreview::Unreadable`].
    Unreadable {
        /// Why the preview couldn't be read back.
        reason: PreviewUnreadableReasonDto,
    },
}

impl From<&PreviewUnreadable> for PreviewUnreadableReasonDto {
    fn from(value: &PreviewUnreadable) -> Self {
        match value {
            PreviewUnreadable::TooLarge => Self::TooLarge,
            PreviewUnreadable::UnsupportedFormat => Self::UnsupportedFormat,
            PreviewUnreadable::Io(_) => Self::Io,
        }
    }
}

impl From<&ModPreview> for ModPreviewDto {
    fn from(value: &ModPreview) -> Self {
        match value {
            ModPreview::Image(output) => {
                let format: TextureFormatDto = output.texture.format.into();
                Self::Image {
                    data_url: encode_data_url(format, &output.texture.bytes),
                    bytes: output.texture.bytes.len(),
                    format,
                }
            }
            ModPreview::Absent => Self::Absent,
            ModPreview::NotOnDisk => Self::NotOnDisk,
            ModPreview::Unreadable(reason) => Self::Unreadable {
                reason: reason.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#[serde(tag = "kind", rename_all = "camelCase")]` on a tagged
    /// enum renames variant names, never a struct variant's own fields
    /// (`apps/desktop/CLAUDE.md`'s own documented trap) — this pins the
    /// wire shape directly, so a future field added to `ModAboutDto`/
    /// `ModPreviewDto` without its own `#[serde(rename)]` fails here
    /// instead of only showing up as a silent snake_case leak in the
    /// generated `.ts` file.
    #[test]
    fn mod_about_read_serializes_every_field_as_camel_case() {
        let dto = ModAboutDto::Read {
            mod_version: Some("1.0".to_string()),
            mod_icon_path: Some("UI/Icon".to_string()),
            description: None,
            homepage: None,
        };

        let json = serde_json::to_value(&dto).expect("serializable");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "read",
                "modVersion": "1.0",
                "modIconPath": "UI/Icon",
                "description": null,
                "homepage": null,
            })
        );
    }

    #[test]
    fn mod_preview_image_serializes_every_field_as_camel_case() {
        let dto = ModPreviewDto::Image {
            data_url: "data:image/png;base64,AA==".to_string(),
            format: TextureFormatDto::Png,
            bytes: 2,
        };

        let json = serde_json::to_value(&dto).expect("serializable");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "image",
                "dataUrl": "data:image/png;base64,AA==",
                "format": "png",
                "bytes": 2,
            })
        );
    }
}
