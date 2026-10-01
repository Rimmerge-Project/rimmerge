//! DTOs for `list_mods`/`get_mod`/`list_mod_names`.

use std::collections::BTreeMap;

use rim_analyzer::domain::{
    DeclaredOrder, Edge, EdgeStatus, GeneratedKind, GeneratedMarker, ModDependency, ModId, Report,
};
use rim_session::{ModFilter, ModPage, ModSummary};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{EdgeKindDto, EdgeStatusDto, EdgeStrengthDto, SourceDto};
use crate::error::CommandError;

/// Mirrors [`GeneratedKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GeneratedKindDto {
    /// See [`GeneratedKind::Merge`].
    Merge,
    /// See [`GeneratedKind::Patch`].
    Patch,
    /// See [`GeneratedKind::Assignment`].
    Assignment,
}

impl From<GeneratedKind> for GeneratedKindDto {
    fn from(value: GeneratedKind) -> Self {
        match value {
            GeneratedKind::Merge => Self::Merge,
            GeneratedKind::Patch => Self::Patch,
            GeneratedKind::Assignment => Self::Assignment,
        }
    }
}

/// A mod's `rimmerge.json` marker. Mirrors [`GeneratedMarker`]; `None`
/// (rather than this DTO being itself optional at the call site) means the
/// mod carries no marker at all — an ordinary, non-generated mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GeneratedMarkerDto {
    /// Which kind of generated mod this is.
    pub kind: GeneratedKindDto,
    /// The compat patch's id, when `kind` is [`GeneratedKindDto::Patch`];
    /// the assignment project's id, when `kind` is
    /// [`GeneratedKindDto::Assignment`] — same slot, see
    /// [`GeneratedMarker::patch_id`].
    pub patch_id: Option<String>,
    /// The mods this generated mod declares itself about; `None` =
    /// unrestricted (the profile merge mod).
    pub scope: Option<Vec<String>>,
}

impl From<&GeneratedMarker> for GeneratedMarkerDto {
    fn from(value: &GeneratedMarker) -> Self {
        Self {
            kind: value.kind.into(),
            patch_id: value.patch_id.clone(),
            scope: value
                .scope
                .as_ref()
                .map(|scope| scope.iter().map(|id| id.as_str().to_string()).collect()),
        }
    }
}

/// One row of `list_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModSummaryDto {
    /// The mod's id.
    pub mod_id: String,
    /// Its display name — a missing row's own id text (see `missing`).
    pub name: String,
    /// Where its files come from. `null` only for a `missing` row — it
    /// was never found on disk, so there's no `About.xml` to have read
    /// one from.
    pub source: Option<SourceDto>,
    /// Every tag it currently carries. Always empty for a `missing` row.
    pub tags: Vec<String>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    /// Always `0` for a `missing` row.
    pub hard_dependents: usize,
    /// Active per `ModsConfig.xml` but never found on disk — the Mods page's
    /// own "missing" pill.
    pub missing: bool,
    /// Its `rimmerge.json` marker, when it carries one — `null` for an
    /// ordinary, non-generated mod.
    pub generated: Option<GeneratedMarkerDto>,
    /// Its Steam Workshop published-file id — `null` for a local, Core, or
    /// DLC mod. See [`rim_analyzer::domain::Mod::workshop_id`].
    #[ts(type = "number | null")]
    pub workshop_id: Option<u64>,
}

/// Request shape for `list_mods`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModFilterDto {
    /// Only mods whose id or name contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// Only mods carrying this tag.
    pub tag: Option<String>,
    /// Only mods from this source.
    pub source: Option<SourceDto>,
    /// How many matching mods to skip.
    pub offset: usize,
    /// How many mods to return, capped at
    /// [`rim_session::MAX_PAGE_SIZE`].
    pub limit: usize,
}

impl TryFrom<ModFilterDto> for ModFilter {
    type Error = CommandError;

    fn try_from(value: ModFilterDto) -> Result<Self, Self::Error> {
        Ok(Self {
            search: value.search,
            tag: value.tag.map(rim_resolve::domain::Tag::new).transpose()?,
            source: value.source.map(Into::into),
            offset: value.offset,
            limit: value.limit,
        })
    }
}

/// One page of `list_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModPageDto {
    /// Total mods matching the filter, before paging.
    pub total: usize,
    /// This page's rows.
    pub items: Vec<ModSummaryDto>,
}

impl From<ModPage> for ModPageDto {
    fn from(value: ModPage) -> Self {
        Self {
            total: value.total,
            items: value.items.iter().map(Into::into).collect(),
        }
    }
}

/// [`ModPageDto`], with each row's `generated` marker and `workshop_id`
/// overlaid from `report` — the source [`ModSummary`] doesn't carry either
/// (see [`From<&ModSummary>`](ModSummaryDto)'s own doc comment), so this is
/// what `list_mods_inner` calls instead of the plain [`From`] conversion.
#[must_use]
pub fn mod_page_dto(page: ModPage, report: &Report) -> ModPageDto {
    let generated_by_id: BTreeMap<&ModId, &GeneratedMarker> = report
        .mods
        .iter()
        .filter_map(|m| m.generated.as_ref().map(|g| (&m.id, g)))
        .collect();
    let workshop_id_by_id: BTreeMap<&ModId, Option<u64>> =
        report.mods.iter().map(|m| (&m.id, m.workshop_id)).collect();

    ModPageDto {
        total: page.total,
        items: page
            .items
            .iter()
            .map(|summary| ModSummaryDto {
                generated: generated_by_id.get(&summary.mod_id).map(|g| (*g).into()),
                workshop_id: workshop_id_by_id.get(&summary.mod_id).copied().flatten(),
                ..ModSummaryDto::from(summary)
            })
            .collect(),
    }
}

/// One `modDependencies` entry. Mirrors [`ModDependency`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModDependencyDto {
    /// The required mod.
    pub id: String,
    /// The display name the author gave it, if any.
    pub display_name: Option<String>,
}

impl From<&ModDependency> for ModDependencyDto {
    fn from(value: &ModDependency) -> Self {
        Self {
            id: value.id.as_str().to_string(),
            display_name: value.display_name.clone(),
        }
    }
}

/// A mod's declared load-order hints. Mirrors [`DeclaredOrder`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DeclaredOrderDto {
    /// `loadAfter` entries.
    pub load_after: Vec<String>,
    /// `loadBefore` entries.
    pub load_before: Vec<String>,
    /// `forceLoadAfter` entries.
    pub force_load_after: Vec<String>,
    /// `forceLoadBefore` entries.
    pub force_load_before: Vec<String>,
    /// `modDependencies` entries.
    pub dependencies: Vec<ModDependencyDto>,
    /// `incompatibleWith` entries.
    pub incompatible_with: Vec<String>,
}

impl From<&DeclaredOrder> for DeclaredOrderDto {
    fn from(value: &DeclaredOrder) -> Self {
        fn ids(list: &[rim_analyzer::domain::ModId]) -> Vec<String> {
            list.iter().map(|id| id.as_str().to_string()).collect()
        }

        Self {
            load_after: ids(&value.load_after),
            load_before: ids(&value.load_before),
            force_load_after: ids(&value.force_load_after),
            force_load_before: ids(&value.force_load_before),
            dependencies: value.dependencies.iter().map(Into::into).collect(),
            incompatible_with: ids(&value.incompatible_with),
        }
    }
}

/// One edge touching a mod, evaluated against a chosen order. Mirrors
/// [`Edge`] plus an [`EdgeStatus`] re-evaluated for the selected order
/// (never the report's own cached status, which is fixed to the order
/// active at scan time — see `rim_resolve::evaluate`'s own doc comment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModEdgeDto {
    /// The other mod this edge connects to.
    pub other_mod_id: String,
    /// The kind of edge.
    pub kind: EdgeKindDto,
    /// This edge's effective strength.
    pub strength: EdgeStrengthDto,
    /// The engine's own description.
    pub detail: String,
    /// Whether the selected order satisfies it.
    pub status: EdgeStatusDto,
}

impl ModEdgeDto {
    /// Builds a [`ModEdgeDto`] for an edge where `mod_id` is the `after`
    /// side (an edge pointing *into* it from `other`).
    #[must_use]
    pub fn incoming(edge: &Edge, status: EdgeStatus) -> Self {
        Self {
            other_mod_id: edge.before.as_str().to_string(),
            kind: edge.kind.into(),
            strength: edge.strength().into(),
            detail: edge.detail.clone(),
            status: status.into(),
        }
    }

    /// Builds a [`ModEdgeDto`] for an edge where `mod_id` is the `before`
    /// side (an edge this mod points out to `other`).
    #[must_use]
    pub fn outgoing(edge: &Edge, status: EdgeStatus) -> Self {
        Self {
            other_mod_id: edge.after.as_str().to_string(),
            kind: edge.kind.into(),
            strength: edge.strength().into(),
            detail: edge.detail.clone(),
            status: status.into(),
        }
    }
}

/// One mod's full detail. Mirrors [`rim_analyzer::domain::Mod`] plus its edges (grouped by
/// direction, evaluated against the selected order), current tags, and
/// the canonical keys of every live finding naming it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ModDetailDto {
    /// The mod's id.
    pub mod_id: String,
    /// Its display name.
    pub name: String,
    /// Its listed authors.
    pub authors: Vec<String>,
    /// Its `About.xml` url, if any.
    pub url: Option<String>,
    /// Where its files come from.
    pub source: SourceDto,
    /// Its `supportedVersions` list.
    pub supported_versions: Vec<String>,
    /// Its declared load-order hints.
    pub declared: DeclaredOrderDto,
    /// Every tag it currently carries.
    pub tags: Vec<String>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    pub hard_dependents: usize,
    /// Whether it looks like a shared framework other mods build on.
    pub is_framework_candidate: bool,
    /// Its `rimmerge.json` marker, when it carries one — `null` for an
    /// ordinary, non-generated mod.
    pub generated: Option<GeneratedMarkerDto>,
    /// Its Steam Workshop published-file id — `null` for a local, Core, or
    /// DLC mod. See [`rim_analyzer::domain::Mod::workshop_id`].
    #[ts(type = "number | null")]
    pub workshop_id: Option<u64>,
    /// Edges pointing into this mod (it is the `after` side).
    pub edges_in: Vec<ModEdgeDto>,
    /// Edges this mod points out to another (it is the `before` side).
    pub edges_out: Vec<ModEdgeDto>,
    /// Canonical keys of every live finding naming this mod, capped at
    /// [`rim_session::MAX_PAGE_SIZE`].
    pub finding_keys: Vec<String>,
    /// Total live findings naming this mod, before the
    /// [`rim_session::MAX_PAGE_SIZE`] cap on `finding_keys`.
    pub finding_keys_total: usize,
}

/// `list_mod_names`'s result: every active mod's id (as-is) mapped to its
/// display name, plus — for a `_steam`-suffixed active id — a second entry
/// under its base id, so a lookup by either resolves. Wraps
/// `BTreeMap<String, String>` rather than exposing it directly so the
/// exported TS type reads as a clean `Record<string, string>` (ts-rs's
/// built-in map support renders `{ [key in string]?: string }` instead).
// No `#[serde(transparent)]`: a single-field tuple struct already
// serializes as its inner value (see `dto::assignment::FieldSpecMapDto`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/generated/",
    type = "Record<string, string>"
)]
pub struct ModNamesDto(pub BTreeMap<String, String>);

impl From<&ModSummary> for ModSummaryDto {
    /// [`ModSummary`] doesn't carry `generated` or `workshop_id` (it's a
    /// `rim_session` listing projection, not the full
    /// [`rim_analyzer::domain::Mod`]) — this leaves them `None`;
    /// [`mod_page_dto`] overlays both real values from the session's
    /// report afterward, since only it holds both.
    fn from(value: &ModSummary) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            name: value.name.clone(),
            source: value.source.map(Into::into),
            tags: value.tags.iter().map(ToString::to_string).collect(),
            hard_dependents: value.hard_dependents,
            missing: value.missing,
            generated: None,
            workshop_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use rim_analyzer::domain::{Mod, REPORT_SCHEMA_VERSION, ReportMetadata, Source};

    use super::*;

    /// Same pin as `dto::assignment::newtype_map_dtos_serialize_as_the_bare_map`:
    /// the frontend reads this as a plain `Record<string, string>`.
    #[test]
    fn mod_names_dto_serializes_as_the_bare_map() {
        let dto = ModNamesDto(BTreeMap::from([(
            "fixture.moda".to_string(),
            "Mod A".to_string(),
        )]));
        assert_eq!(
            serde_json::to_value(&dto).expect("serializable"),
            serde_json::json!({"fixture.moda": "Mod A"})
        );
    }

    fn edge(after: &str, before: &str) -> Edge {
        Edge {
            after: ModId::new(after),
            before: ModId::new(before),
            kind: rim_analyzer::domain::EdgeKind::AssemblyRef,
            detail: "AssemblyRef".to_string(),
            load_time: true,
            subject: None,
        }
    }

    #[test]
    fn incoming_edge_dto_names_the_before_side_as_the_other_mod() {
        let e = edge("addon", "framework");
        let dto = ModEdgeDto::incoming(&e, EdgeStatus::Satisfied);
        assert_eq!(dto.other_mod_id, "framework");
        assert_eq!(dto.status, EdgeStatusDto::Satisfied);
    }

    #[test]
    fn outgoing_edge_dto_names_the_after_side_as_the_other_mod() {
        let e = edge("addon", "framework");
        let dto = ModEdgeDto::outgoing(&e, EdgeStatus::Violated);
        assert_eq!(dto.other_mod_id, "addon");
        assert_eq!(dto.status, EdgeStatusDto::Violated);
    }

    fn bare_mod(id: &str) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            url: None,
            path: PathBuf::from(id),
            source: Source::Local,
            supported_versions: Vec::new(),
            declared: DeclaredOrder::default(),
            loaded_folders: Vec::new(),
            hard_dependents: 0,
            soft_dependents: 0,
            awareness_dependents: 0,
            is_framework_candidate: false,
            generated: None,
            workshop_id: None,
            load_folders_version_matched: None,
        }
    }

    fn report_with(mods: Vec<Mod>) -> Report {
        Report {
            metadata: ReportMetadata {
                schema_version: REPORT_SCHEMA_VERSION,
                game_dir: PathBuf::new(),
                workshop_dir: PathBuf::new(),
                mods_config: PathBuf::new(),
                game_version: "1.6".to_string(),
                generated_at: String::new(),
                active_mod_count: mods.len(),
                scanned_mod_count: mods.len(),
                mods_with_assemblies: 0,
                mods_with_patches: 0,
                mods_with_defs: 0,
                total_defs_indexed: 0,
                distinct_texture_paths: 0,
                discovered_mod_count: mods.len(),
                core_resource_texture_count: 0,
            },
            mods,
            edges: Vec::new(),
            conflicts: Vec::new(),
            constraints: Vec::new(),
            undeclared_hard_dependencies: Vec::new(),
            missing_mods: Vec::new(),
            missing_dependencies: Vec::new(),
            incompatible_active_pairs: Vec::new(),
            unsupported_version_mods: Vec::new(),
            unresolved_find_mod_names: Vec::new(),
            find_mod_names_using_package_id: Vec::new(),
            warnings: Vec::new(),
            mod_costs: Vec::new(),
            inactive_mods: Vec::new(),
        }
    }

    fn summary_for(mod_entry: &Mod) -> ModSummary {
        ModSummary {
            mod_id: mod_entry.id.clone(),
            name: mod_entry.name.clone(),
            source: Some(mod_entry.source),
            tags: BTreeSet::new(),
            hard_dependents: 0,
            missing: false,
        }
    }

    #[test]
    fn mod_page_dto_overlays_the_generated_marker_from_the_report() {
        let mut generated_mod = bare_mod("sample.abcompat");
        generated_mod.generated = Some(GeneratedMarker {
            kind: GeneratedKind::Patch,
            patch_id: Some("3f9a1c02be77".to_string()),
            scope: Some(BTreeSet::from([ModId::new("fixture.moda")])),
        });
        let plain_mod = bare_mod("someone.plain");
        let report = report_with(vec![generated_mod.clone(), plain_mod.clone()]);
        let page = ModPage {
            total: 2,
            items: vec![summary_for(&generated_mod), summary_for(&plain_mod)],
        };

        let dto = mod_page_dto(page, &report);

        let generated_row = dto
            .items
            .iter()
            .find(|item| item.mod_id == "sample.abcompat")
            .expect("generated mod must be in the page");
        let marker = generated_row
            .generated
            .as_ref()
            .expect("its marker must be overlaid");
        assert_eq!(marker.kind, GeneratedKindDto::Patch);
        assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
        assert_eq!(marker.scope, Some(vec!["fixture.moda".to_string()]));

        let plain_row = dto
            .items
            .iter()
            .find(|item| item.mod_id == "someone.plain")
            .expect("plain mod must be in the page");
        assert!(plain_row.generated.is_none());
    }
}
