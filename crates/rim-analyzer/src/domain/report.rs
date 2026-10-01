//! [`Report`]: the full analysis result, serialized to JSON for a later
//! sorter to consume and rendered as a text summary for stdout.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::conflict::Conflict;
use super::constraint::Constraint;
use super::declared_order::ModDependency;
use super::edge::{Edge, EdgeReport};
use super::inactive_mod::InactiveMod;
use super::mod_cost::ModCost;
use super::mod_entry::Mod;
use super::mod_id::ModId;
use super::warning::Warning;

/// The `Report` contract's own version, bumped whenever a field is added,
/// removed, or reinterpreted — independent of the store-version numbers
/// `rim-io`'s persisted files (`rules.json`, `decisions.json`, ...) carry.
/// It is bumped for three kinds of change:
///
/// - **New variants** (`EdgeKind`, `EdgeStrength`, `Conflict`): a consumer
///   holding an old, non-exhaustive-by-design `match` needs to know a
///   cached report might carry variants it doesn't recognize. New variants
///   are appended at the end of their enum so no existing variant's
///   relative `Ord`/`Hash` position moves.
/// - **New fields with a safe default** (`Report::mod_costs`,
///   `Report::inactive_mods`, `ReportMetadata::discovered_mod_count`,
///   `ModCost::nameless_def_count`): each is `#[serde(default)]`-backed,
///   and the empty value reads back as a correct "not recorded by this
///   scan", not a misreport. `Edge.subject` is in the same category:
///   `None` is the right value for an edge that predates the field.
/// - **Changes a default cannot represent**: membership, grouping, or
///   strength changes to `edges`/`conflicts`/`warnings`; facts that live
///   only on in-memory types (`PatchOp`, `ScannedMod`,
///   `RuntimePatchTarget`) and never reach `Report`'s JSON bytes; or a
///   field such as `Mod::load_folders_version_matched`, whose `None`
///   default reads as "ships no `LoadFolders.xml`" and would misreport a
///   mod whose file has no matching version. For these the only correct
///   response to a stale report is to regenerate it.
///
/// **Disclosed, not enforced**: the version is a human tripwire, not a
/// read-time guard. Nothing in this workspace rejects a stale report on
/// `schema_version` grounds; every real `Session` comes from a fresh
/// `AnalyzerScanner` scan, never a deserialized cache, so the hazard is
/// latent. A future caller that loads cached reports must check the
/// version itself.
pub const REPORT_SCHEMA_VERSION: u32 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    /// See [`REPORT_SCHEMA_VERSION`]. `#[serde(default)]` so a report
    /// cached before this field existed (schema version 1, implicit)
    /// still deserializes — reading `0` back as "version 1" is a caller
    /// concern if one ever needs to branch on it; nothing in this crate
    /// does yet.
    #[serde(default)]
    pub schema_version: u32,
    pub game_dir: PathBuf,
    pub workshop_dir: PathBuf,
    pub mods_config: PathBuf,
    pub game_version: String,
    /// RFC 3339 timestamp of when the analysis ran.
    pub generated_at: String,
    pub active_mod_count: usize,
    pub scanned_mod_count: usize,
    pub mods_with_assemblies: usize,
    pub mods_with_patches: usize,
    pub mods_with_defs: usize,
    /// Total def entries indexed across every scanned mod (raw count, not
    /// deduplicated by owner).
    pub total_defs_indexed: usize,
    /// Distinct normalized texture paths across every scanned mod.
    pub distinct_texture_paths: usize,
    /// Every mod directory discovery found on disk,
    /// active or not — see [`crate::domain::ScanOutput::discovered_mod_count`]'s
    /// own doc comment for exactly what this does and doesn't sum with.
    /// `#[serde(default)]` so a report cached before this field existed
    /// still deserializes, reading `0` back as "not recorded by this
    /// scan".
    #[serde(default)]
    pub discovered_mod_count: usize,
    /// The number of entries [`super::ScanOutput::core_resource_textures`]
    /// found — surfaced so the real-install tier can assert the floor
    /// directly against the report rather than re-deriving it.
    /// `#[serde(default)]` so a report cached before this field existed
    /// still deserializes, reading back `0` — the same value a scan whose
    /// `globalgamemanagers` read genuinely failed produces, which is the
    /// correct reading for either case (in both, `missing_texture_path` was
    /// disabled for the scan — see
    /// `analysis::indices::MIN_CORE_RESOURCE_TEXTURES`).
    #[serde(default)]
    pub core_resource_texture_count: usize,
}

/// An active mod's declared dependency that is not itself active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDependency {
    pub mod_id: ModId,
    pub dependency: ModDependency,
}

/// Two active mods that declare each other (or one declares the other)
/// incompatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncompatiblePair {
    pub a: ModId,
    pub b: ModId,
}

/// A `PatchOperationFindMod` display name that didn't resolve to any
/// active mod's name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedFindMod {
    pub mod_id: ModId,
    pub display_name: String,
    /// Where this `FindMod` was written — `None` for a report predating
    /// this field. `#[serde(default)]` so an older cached report still
    /// deserializes.
    #[serde(default)]
    pub locator: Option<super::XmlLocator>,
}

/// The complete analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub metadata: ReportMetadata,
    pub mods: Vec<Mod>,
    pub edges: Vec<EdgeReport>,
    pub conflicts: Vec<Conflict>,
    /// Load-order requirements satisfied by any one of several candidate
    /// mods (an assembly name shipped by more than one active mod).
    pub constraints: Vec<Constraint>,
    /// `AssemblyRef` edges with no declared edge of the same direction
    /// between the same two mods.
    pub undeclared_hard_dependencies: Vec<Edge>,
    /// Active mods (per `ModsConfig.xml`) not found on disk.
    pub missing_mods: Vec<ModId>,
    pub missing_dependencies: Vec<MissingDependency>,
    pub incompatible_active_pairs: Vec<IncompatiblePair>,
    /// Active mods whose `supportedVersions` don't list the game version.
    pub unsupported_version_mods: Vec<ModId>,
    pub unresolved_find_mod_names: Vec<UnresolvedFindMod>,
    /// Unresolved `PatchOperationFindMod` names that happen to equal an
    /// active mod's packageId — RimWorld's own `FindMod` matches on
    /// display name only, so these stay genuinely unresolved (no edge is
    /// built), but are worth surfacing separately as a likely authoring
    /// mistake.
    pub find_mod_names_using_package_id: Vec<UnresolvedFindMod>,
    pub warnings: Vec<Warning>,
    /// One row per scanned (active) mod, in the same
    /// order as [`Self::mods`] (scan order, not sorted by [`ModId`] —
    /// matching the convention every other per-mod `Report` vector
    /// already follows). `#[serde(default)]` so a report cached before
    /// this field existed still deserializes, `mod_costs: []` read back
    /// as "not computed by this scan" rather than "this install has no
    /// mods".
    #[serde(default)]
    pub mod_costs: Vec<ModCost>,
    /// Every mod discovery found on disk but that isn't active, sorted by id.
    /// `#[serde(default)]` so a report cached before this field existed
    /// still deserializes, `inactive_mods: []` read back as "not
    /// recorded by this scan" — the same safe-default category
    /// `mod_costs` is in, not the "regenerate rather than trust a
    /// default" category some other schema changes are in.
    #[serde(default)]
    pub inactive_mods: Vec<InactiveMod>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Report` serialized before `mod_costs` existed (no such key at
    /// all, schema version 5) must still deserialize — the contract
    /// `#[serde(default)]` exists to guarantee: an older cached
    /// `report.json` is not a hard break.
    #[test]
    fn a_report_json_from_before_the_mod_costs_field_existed_still_deserializes() {
        let json = r#"{
            "metadata": {
                "schema_version": 5,
                "game_dir": "game",
                "workshop_dir": "workshop",
                "mods_config": "ModsConfig.xml",
                "game_version": "1.6",
                "generated_at": "2026-09-10T00:00:00Z",
                "active_mod_count": 0,
                "scanned_mod_count": 0,
                "mods_with_assemblies": 0,
                "mods_with_patches": 0,
                "mods_with_defs": 0,
                "total_defs_indexed": 0,
                "distinct_texture_paths": 0
            },
            "mods": [],
            "edges": [],
            "conflicts": [],
            "constraints": [],
            "undeclared_hard_dependencies": [],
            "missing_mods": [],
            "missing_dependencies": [],
            "incompatible_active_pairs": [],
            "unsupported_version_mods": [],
            "unresolved_find_mod_names": [],
            "find_mod_names_using_package_id": [],
            "warnings": []
        }"#;

        let report: Report =
            serde_json::from_str(json).expect("must deserialize without `mod_costs`");

        assert!(report.mod_costs.is_empty());
    }

    /// A `Report` serialized before `inactive_mods`/`discovered_mod_count`
    /// existed (schema version 14) must still deserialize — the same
    /// `#[serde(default)]` contract the `mod_costs` test above guards.
    #[test]
    fn a_report_json_from_before_the_inactive_mods_field_existed_still_deserializes() {
        let json = r#"{
            "metadata": {
                "schema_version": 14,
                "game_dir": "game",
                "workshop_dir": "workshop",
                "mods_config": "ModsConfig.xml",
                "game_version": "1.6",
                "generated_at": "2026-09-10T00:00:00Z",
                "active_mod_count": 0,
                "scanned_mod_count": 0,
                "mods_with_assemblies": 0,
                "mods_with_patches": 0,
                "mods_with_defs": 0,
                "total_defs_indexed": 0,
                "distinct_texture_paths": 0
            },
            "mods": [],
            "edges": [],
            "conflicts": [],
            "constraints": [],
            "undeclared_hard_dependencies": [],
            "missing_mods": [],
            "missing_dependencies": [],
            "incompatible_active_pairs": [],
            "unsupported_version_mods": [],
            "unresolved_find_mod_names": [],
            "find_mod_names_using_package_id": [],
            "warnings": []
        }"#;

        let report: Report =
            serde_json::from_str(json).expect("must deserialize without `inactive_mods`");

        assert!(report.inactive_mods.is_empty());
        assert_eq!(report.metadata.discovered_mod_count, 0);
    }
}
