//! DTOs for `get_dashboard`.

use rim_analyzer::domain::{Conflict, EdgeKind, EdgeReport, EdgeStatus, EdgeStrength};
use rim_session::SortProvenance;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::OrderSourceDto;
use super::finding::{FindingKindCountsDto, LedgerStatsDto};
use super::settings::SortTieBreakDto;

/// Total edge counts bucketed by [`EdgeStrength`]. A fixed-field struct
/// (rather than a map keyed by the enum) since the bucket set is small,
/// closed, and stable — simplest shape for both `ts-rs` and the frontend
/// to consume without a runtime key lookup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EdgeStrengthCountsDto {
    /// Edges of [`EdgeStrength::Hard`].
    pub hard: usize,
    /// Edges of [`EdgeStrength::Declared`].
    pub declared: usize,
    /// Edges of [`EdgeStrength::Soft`].
    pub soft: usize,
    /// Edges of [`EdgeStrength::Awareness`].
    pub awareness: usize,
    /// Edges of [`EdgeStrength::Inferred`] — its own dedicated bucket, not
    /// counted alongside `awareness`.
    pub inferred: usize,
}

impl EdgeStrengthCountsDto {
    fn add(&mut self, strength: EdgeStrength) {
        match strength {
            EdgeStrength::Hard => self.hard += 1,
            EdgeStrength::Declared => self.declared += 1,
            EdgeStrength::Soft => self.soft += 1,
            EdgeStrength::Awareness => self.awareness += 1,
            EdgeStrength::Inferred => self.inferred += 1,
        }
    }
}

/// Violated-edge counts bucketed by [`EdgeKind`] (the edge's own source
/// fact — an `AssemblyRef`, a declared `loadAfter`, ...), evaluated
/// against whichever order is selected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EdgeKindCountsDto {
    /// See [`EdgeKind::AssemblyRef`].
    pub assembly_ref: usize,
    /// See [`EdgeKind::ForceLoadAfter`].
    pub force_load_after: usize,
    /// See [`EdgeKind::ForceLoadBefore`].
    pub force_load_before: usize,
    /// See [`EdgeKind::LoadAfter`].
    pub load_after: usize,
    /// See [`EdgeKind::LoadBefore`].
    pub load_before: usize,
    /// See [`EdgeKind::ModDependency`].
    pub mod_dependency: usize,
    /// See [`EdgeKind::FindMod`].
    pub find_mod: usize,
    /// See [`EdgeKind::IfModActive`].
    pub if_mod_active: usize,
    /// See [`EdgeKind::PatchTargetsDef`].
    pub patch_targets_def: usize,
    /// See [`EdgeKind::MayRequire`].
    pub may_require: usize,
    /// See [`EdgeKind::PatchInjectedNode`].
    pub patch_injected_node: usize,
    /// See [`EdgeKind::AssemblyVersionPrecedence`].
    pub assembly_version_precedence: usize,
    /// See [`EdgeKind::UsesType`].
    pub uses_type: usize,
    /// See [`EdgeKind::ParentTemplate`].
    pub parent_template: usize,
    /// See [`EdgeKind::PatchRemovedNode`].
    pub patch_removed_node: usize,
    /// See [`EdgeKind::RetextureAfterOwner`].
    pub retexture_after_owner: usize,
    /// See [`EdgeKind::DefOverrideAfterOrigin`].
    pub def_override_after_origin: usize,
    /// See [`EdgeKind::PatchSelectsInjectedNode`].
    pub patch_selects_injected_node: usize,
    /// See [`EdgeKind::PatchInvalidatesPredicate`].
    pub patch_invalidates_predicate: usize,
    /// See [`EdgeKind::PatchRemovedNodeCosmetic`].
    pub patch_removed_node_cosmetic: usize,
    /// See [`EdgeKind::ReplaceDiscardsAddition`].
    pub replace_discards_addition: usize,
}

impl EdgeKindCountsDto {
    fn add(&mut self, kind: EdgeKind) {
        match kind {
            EdgeKind::AssemblyRef => self.assembly_ref += 1,
            EdgeKind::ForceLoadAfter => self.force_load_after += 1,
            EdgeKind::ForceLoadBefore => self.force_load_before += 1,
            EdgeKind::LoadAfter => self.load_after += 1,
            EdgeKind::LoadBefore => self.load_before += 1,
            EdgeKind::ModDependency => self.mod_dependency += 1,
            EdgeKind::FindMod => self.find_mod += 1,
            EdgeKind::IfModActive => self.if_mod_active += 1,
            EdgeKind::PatchTargetsDef => self.patch_targets_def += 1,
            EdgeKind::MayRequire => self.may_require += 1,
            EdgeKind::PatchInjectedNode => self.patch_injected_node += 1,
            EdgeKind::AssemblyVersionPrecedence => self.assembly_version_precedence += 1,
            EdgeKind::UsesType => self.uses_type += 1,
            EdgeKind::ParentTemplate => self.parent_template += 1,
            EdgeKind::PatchRemovedNode => self.patch_removed_node += 1,
            EdgeKind::RetextureAfterOwner => self.retexture_after_owner += 1,
            EdgeKind::DefOverrideAfterOrigin => self.def_override_after_origin += 1,
            EdgeKind::PatchSelectsInjectedNode => self.patch_selects_injected_node += 1,
            EdgeKind::PatchInvalidatesPredicate => self.patch_invalidates_predicate += 1,
            EdgeKind::PatchRemovedNodeCosmetic => self.patch_removed_node_cosmetic += 1,
            EdgeKind::ReplaceDiscardsAddition => self.replace_discards_addition += 1,
        }
    }
}

/// Builds the edge-strength and violated-by-kind counts for `edges`,
/// re-evaluating each edge's status against `order` rather than trusting
/// the report's own cached status (fixed to the order active at scan
/// time — see `rim_resolve::evaluate`'s doc comment).
#[must_use]
pub fn count_edges(
    edges: &[EdgeReport],
    order: &rim_analyzer::domain::LoadOrder,
) -> (EdgeStrengthCountsDto, EdgeKindCountsDto) {
    let mut by_strength = EdgeStrengthCountsDto::default();
    let mut violated_by_kind = EdgeKindCountsDto::default();
    for report in edges {
        by_strength.add(report.edge.strength());
        if rim_resolve::evaluate::edge_status(&report.edge, order) == EdgeStatus::Violated {
            violated_by_kind.add(report.edge.kind);
        }
    }
    (by_strength, violated_by_kind)
}

/// Conflict counts bucketed by [`Conflict`] variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ConflictKindCountsDto {
    /// See [`Conflict::DefOverride`].
    pub def_override: usize,
    /// See [`Conflict::PatchCollision`].
    pub patch_collision: usize,
    /// See [`Conflict::TextureOverride`].
    pub texture_override: usize,
    /// See [`Conflict::DuplicateAssembly`].
    pub duplicate_assembly: usize,
    /// See [`Conflict::LikelyDuplicateMod`].
    pub likely_duplicate_mod: usize,
    /// See [`Conflict::DuplicateTemplateName`].
    pub duplicate_template_name: usize,
    /// See [`Conflict::KeyedTranslationCollision`].
    pub keyed_translation_collision: usize,
    /// See [`Conflict::SoundOverride`].
    pub sound_override: usize,
    /// See [`Conflict::RuntimePatchCollision`].
    pub runtime_patch_collision: usize,
    /// See [`Conflict::MissingTexturePath`].
    pub missing_texture_path: usize,
    /// See [`Conflict::TranspilerCollision`].
    pub transpiler_collision: usize,
    /// See [`Conflict::UndecodableTexture`].
    pub undecodable_texture: usize,
    /// See [`Conflict::BrokenInheritance`].
    pub broken_inheritance: usize,
    /// See [`Conflict::NearMissModReference`].
    pub near_miss_mod_reference: usize,
    /// See [`Conflict::DiscardedAddition`].
    pub discarded_addition: usize,
    /// See [`Conflict::DanglingDefReference`].
    pub dangling_def_reference: usize,
}

impl From<&[Conflict]> for ConflictKindCountsDto {
    fn from(conflicts: &[Conflict]) -> Self {
        let mut counts = Self::default();
        for conflict in conflicts {
            match conflict {
                Conflict::DefOverride(_) => counts.def_override += 1,
                Conflict::PatchCollision(_) => counts.patch_collision += 1,
                Conflict::TextureOverride(_) => counts.texture_override += 1,
                Conflict::DuplicateAssembly(_) => counts.duplicate_assembly += 1,
                Conflict::LikelyDuplicateMod(_) => counts.likely_duplicate_mod += 1,
                Conflict::DuplicateTemplateName(_) => counts.duplicate_template_name += 1,
                Conflict::KeyedTranslationCollision(_) => counts.keyed_translation_collision += 1,
                Conflict::SoundOverride(_) => counts.sound_override += 1,
                Conflict::RuntimePatchCollision(_) => counts.runtime_patch_collision += 1,
                Conflict::MissingTexturePath(_) => counts.missing_texture_path += 1,
                Conflict::TranspilerCollision(_) => counts.transpiler_collision += 1,
                Conflict::UndecodableTexture(_) => counts.undecodable_texture += 1,
                Conflict::BrokenInheritance(_) => counts.broken_inheritance += 1,
                Conflict::NearMissModReference(_) => counts.near_miss_mod_reference += 1,
                Conflict::DiscardedAddition(_) => counts.discarded_addition += 1,
                Conflict::DanglingDefReference(_) => counts.dangling_def_reference += 1,
            }
        }
        counts
    }
}

/// Which settings produced the suggested order, for display next to
/// "mods that move" (the dashboard tile and the apply dialog's own line)
/// and the why-panel — a large disturbance from the first `Rebuild` of a
/// list RimSort produced is explained rather than alarming
/// Mirrors [`SortProvenance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SortProvenanceDto {
    /// Which base key an unconstrained mod's emission started from.
    pub tie_break: SortTieBreakDto,
    /// Whether imported pair rules fed the sorter.
    pub use_imported_pairs: bool,
    /// Whether imported placement rules fed the sorter.
    pub use_imported_placements: bool,
}

impl From<SortProvenance> for SortProvenanceDto {
    fn from(value: SortProvenance) -> Self {
        Self {
            tie_break: value.tie_break.into(),
            use_imported_pairs: value.use_imported_pairs,
            use_imported_placements: value.use_imported_placements,
        }
    }
}

/// A [`LedgerStatsDto`] for each [`rim_resolve::domain::OrderSource`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LedgerStatsPairDto {
    /// Ledger stats for [`rim_resolve::domain::OrderSource::Current`].
    pub current: LedgerStatsDto,
    /// Ledger stats for [`rim_resolve::domain::OrderSource::Suggested`].
    pub suggested: LedgerStatsDto,
}

/// The dashboard's summary counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DashboardDto {
    /// How many active mods are loaded.
    pub mod_count: usize,
    /// Total edges, bucketed by strength.
    pub edges_by_strength: EdgeStrengthCountsDto,
    /// Violated edges under the selected order, bucketed by kind.
    pub edges_violated_by_source: EdgeKindCountsDto,
    /// File-collision conflicts, bucketed by kind.
    pub conflicts_by_kind: ConflictKindCountsDto,
    /// Ledger stats for both order sources.
    pub ledger_stats: LedgerStatsPairDto,
    /// Needs-input findings in the selected order's ledger, bucketed by
    /// kind — the dashboard's "top needs-input kinds" summary.
    pub needs_input_by_kind: FindingKindCountsDto,
    /// How many mods change position between the current and suggested
    /// order.
    pub moved_mods: usize,
    /// Which order is currently selected.
    pub selected: OrderSourceDto,
    /// Whether `ModsConfig.xml` (as last scanned or written) already lists
    /// the suggested order — [`rim_session::Session::file_matches`].
    pub file_matches_suggested: bool,
    /// Which settings produced the suggested order — see
    /// [`SortProvenanceDto`].
    pub sort_provenance: SortProvenanceDto,
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::{Edge, EdgeStatus, LoadOrder, ModId};

    use super::*;

    fn edge_report(after: &str, before: &str, kind: EdgeKind, load_time: bool) -> EdgeReport {
        EdgeReport {
            edge: Edge {
                after: ModId::new(after),
                before: ModId::new(before),
                kind,
                detail: String::new(),
                load_time,
                subject: None,
            },
            status: EdgeStatus::Unevaluated,
        }
    }

    #[test]
    fn count_edges_buckets_by_strength_and_counts_violations_by_kind() {
        let edges = vec![
            // Hard, and satisfied: "wraps" loads after "framework", and
            // "framework" is indeed first in the order below.
            edge_report("wraps", "framework", EdgeKind::AssemblyRef, true),
            // Declared, and violated: "addon" must load after "later",
            // but "addon" is first in the order below.
            edge_report("addon", "later", EdgeKind::LoadAfter, true),
        ];
        let order = LoadOrder::new(vec![
            ModId::new("framework"),
            ModId::new("wraps"),
            ModId::new("addon"),
            ModId::new("later"),
        ]);

        let (by_strength, violated_by_kind) = count_edges(&edges, &order);

        assert_eq!(by_strength.hard, 1);
        assert_eq!(by_strength.declared, 1);
        assert_eq!(
            violated_by_kind.load_after, 1,
            "the declared edge is violated"
        );
        assert_eq!(
            violated_by_kind.assembly_ref, 0,
            "the hard edge is satisfied, so it must not be counted as violated"
        );
    }

    #[test]
    fn conflict_kind_counts_dto_buckets_every_conflict_kind() {
        let conflicts = vec![rim_analyzer::domain::Conflict::DuplicateAssembly(
            rim_analyzer::domain::DuplicateAssembly {
                assembly_name: "Shared.dll".to_string(),
                owners: vec![ModId::new("a"), ModId::new("b")],
                versions: vec![
                    (
                        ModId::new("a"),
                        rim_analyzer::domain::AssemblyVersion::default(),
                    ),
                    (
                        ModId::new("b"),
                        rim_analyzer::domain::AssemblyVersion::default(),
                    ),
                ],
                first_loaded: ModId::new("a"),
            },
        )];
        let counts: ConflictKindCountsDto = conflicts.as_slice().into();
        assert_eq!(counts.duplicate_assembly, 1);
        assert_eq!(counts.def_override, 0);
    }
}
