//! DTOs for `list_inactive_mods`/`plan_activate_mods`/`activate_mods`/
//! `plan_deactivate_mods`/`deactivate_mods`/`get_pending_active_changes`/
//! `rescan_project`.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_session::{ActivatePlan, ActiveSetDiff, DeactivatePlan, PendingActiveChanges};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn id_strings(ids: &[ModId]) -> Vec<String> {
    ids.iter().map(|id| id.as_str().to_string()).collect()
}

fn id_map(map: &BTreeMap<ModId, Vec<ModId>>) -> BTreeMap<String, Vec<String>> {
    map.iter()
        .map(|(id, deps)| (id.as_str().to_string(), id_strings(deps)))
        .collect()
}

/// Mirrors [`ActiveSetDiff`]: what a diff added and removed, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ActiveSetDiffDto {
    /// Present in the newer set, absent from the older one.
    pub added: Vec<String>,
    /// Present in the older set, absent from the newer one.
    pub removed: Vec<String>,
}

impl From<&ActiveSetDiff> for ActiveSetDiffDto {
    fn from(value: &ActiveSetDiff) -> Self {
        Self {
            added: id_strings(&value.added),
            removed: id_strings(&value.removed),
        }
    }
}

/// Mirrors [`PendingActiveChanges`]. `get_pending_active_changes`'s
/// result, and `activate_mods`/`deactivate_mods`/`rescan_project`'s own
/// updated reading of it — a non-empty `unscanned` is what the Rescan
/// banner reads, a non-empty `unapplied` is what the apply dialog warns
/// about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PendingActiveChangesDto {
    /// The working set vs. the last scan's own active list.
    pub unscanned: ActiveSetDiffDto,
    /// The last scan's own active list vs. `ModsConfig.xml` on disk.
    pub unapplied: ActiveSetDiffDto,
}

impl From<&PendingActiveChanges> for PendingActiveChangesDto {
    fn from(value: &PendingActiveChanges) -> Self {
        Self {
            unscanned: (&value.unscanned).into(),
            unapplied: (&value.unapplied).into(),
        }
    }
}

/// Request shape for `plan_activate_mods`/`activate_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ActivateRequestDto {
    /// The mods to activate.
    pub ids: Vec<String>,
    /// Whether to also activate each requested mod's own inactive
    /// dependency closure.
    pub with_dependencies: bool,
}

impl ActivateRequestDto {
    /// Parses [`Self::ids`] into [`ModId`]s — infallible, like every
    /// other id field crossing this boundary
    /// ([`rim_analyzer::domain::ModId::new`] never fails).
    #[must_use]
    pub fn mod_ids(&self) -> Vec<ModId> {
        self.ids.iter().map(ModId::new).collect()
    }
}

/// Mirrors [`ActivatePlan`]. `plan_activate_mods`'s result — what
/// `activate_mods` would do, shown in a confirm dialog before it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ActivatePlanDto {
    /// Every id that would be appended, in order: each requested id's own
    /// inactive dependency closure (dependencies first), then the id
    /// itself.
    pub to_add: Vec<String>,
    /// A requested (or closure-walked) mod's own declared dependencies
    /// that resolve to nothing usable on disk, keyed by the mod that
    /// declares them.
    pub unresolvable_dependencies: BTreeMap<String, Vec<String>>,
    /// Requested ids already active — skipped, not re-added.
    pub already_active: Vec<String>,
}

impl From<&ActivatePlan> for ActivatePlanDto {
    fn from(value: &ActivatePlan) -> Self {
        Self {
            to_add: id_strings(&value.to_add),
            unresolvable_dependencies: id_map(&value.unresolvable_dependencies),
            already_active: id_strings(&value.already_active),
        }
    }
}

/// Request shape for `plan_deactivate_mods`/`deactivate_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DeactivateRequestDto {
    /// The mods to deactivate.
    pub ids: Vec<String>,
}

impl DeactivateRequestDto {
    /// See [`ActivateRequestDto::mod_ids`].
    #[must_use]
    pub fn mod_ids(&self) -> Vec<ModId> {
        self.ids.iter().map(ModId::new).collect()
    }
}

/// Mirrors [`DeactivatePlan`]. `plan_deactivate_mods`'s result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DeactivatePlanDto {
    /// Every requested id that would actually be removed.
    pub to_remove: Vec<String>,
    /// Per removed id, every other currently-active mod that still
    /// depends on it (declared `modDependencies` or a Hard-strength
    /// edge) — a warning, never a block.
    pub dependents_still_active: BTreeMap<String, Vec<String>>,
    /// Requested ids refused because they're Core.
    pub refused: Vec<String>,
}

impl From<&DeactivatePlan> for DeactivatePlanDto {
    fn from(value: &DeactivatePlan) -> Self {
        Self {
            to_remove: id_strings(&value.to_remove),
            dependents_still_active: id_map(&value.dependents_still_active),
            refused: id_strings(&value.refused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_request_dto_parses_ids_into_mod_ids() {
        let request = ActivateRequestDto {
            ids: vec!["a.mod".to_string(), "b.mod".to_string()],
            with_dependencies: true,
        };

        assert_eq!(
            request.mod_ids(),
            vec![ModId::new("a.mod"), ModId::new("b.mod")]
        );
    }

    #[test]
    fn active_set_diff_dto_maps_added_and_removed() {
        let diff = ActiveSetDiff {
            added: vec![ModId::new("a")],
            removed: vec![ModId::new("b")],
        };

        let dto: ActiveSetDiffDto = (&diff).into();

        assert_eq!(dto.added, vec!["a".to_string()]);
        assert_eq!(dto.removed, vec!["b".to_string()]);
    }

    #[test]
    fn activate_plan_dto_maps_unresolvable_dependencies_by_id() {
        let mut plan = ActivatePlan {
            to_add: vec![ModId::new("a")],
            ..ActivatePlan::default()
        };
        plan.unresolvable_dependencies
            .insert(ModId::new("a"), vec![ModId::new("ghost")]);

        let dto: ActivatePlanDto = (&plan).into();

        assert_eq!(dto.to_add, vec!["a".to_string()]);
        assert_eq!(
            dto.unresolvable_dependencies.get("a"),
            Some(&vec!["ghost".to_string()])
        );
    }

    #[test]
    fn deactivate_plan_dto_maps_dependents_by_id() {
        let mut plan = DeactivatePlan {
            to_remove: vec![ModId::new("target")],
            ..DeactivatePlan::default()
        };
        plan.dependents_still_active
            .insert(ModId::new("target"), vec![ModId::new("dependent")]);

        let dto: DeactivatePlanDto = (&plan).into();

        assert_eq!(dto.to_remove, vec!["target".to_string()]);
        assert_eq!(
            dto.dependents_still_active.get("target"),
            Some(&vec!["dependent".to_string()])
        );
    }
}
