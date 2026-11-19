//! DTOs for `list_inactive_mods`/`plan_activate_mods`/`activate_mods`/
//! `plan_deactivate_mods`/`deactivate_mods`/`get_pending_active_changes`/
//! `rescan_project`.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_session::use_cases::{ActivateOutcome, MoveOutcome};
use rim_session::{ActivatePlan, ActiveSetDiff, Anchor, DeactivatePlan, PendingActiveChanges};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::OrderSourceDto;

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
    /// The current order lists the file's mods in another sequence: a
    /// settled reorder that an Apply would write. `unapplied` is a
    /// membership diff and stays empty for it.
    pub order_differs_from_file: bool,
}

impl From<&PendingActiveChanges> for PendingActiveChangesDto {
    fn from(value: &PendingActiveChanges) -> Self {
        Self {
            unscanned: (&value.unscanned).into(),
            unapplied: (&value.unapplied).into(),
            order_differs_from_file: value.order_differs_from_file,
        }
    }
}

/// Where a move or an anchored activation puts its mods, by a neighbour's
/// id. Mirrors [`Anchor`]; a neighbour that is no longer active is refused
/// (`move_anchor_not_active`), never clamped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum AnchorDto {
    /// Directly before this active mod.
    #[serde(rename_all = "camelCase")]
    Before {
        /// The neighbour's exact id.
        mod_id: String,
    },
    /// Directly after this active mod.
    #[serde(rename_all = "camelCase")]
    After {
        /// The neighbour's exact id.
        mod_id: String,
    },
    /// At the start of the list.
    Start,
    /// At the end of the list.
    End,
}

impl From<AnchorDto> for Anchor {
    fn from(value: AnchorDto) -> Self {
        match value {
            AnchorDto::Before { mod_id } => Self::Before(ModId::new(mod_id)),
            AnchorDto::After { mod_id } => Self::After(ModId::new(mod_id)),
            AnchorDto::Start => Self::Start,
            AnchorDto::End => Self::End,
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
    /// Where the activated mods (the closure first, then the requested
    /// ones) go. `null` is the default placement: a mod that was active
    /// before goes back to its place in the current order, any other mod to
    /// the end.
    pub anchor: Option<AnchorDto>,
}

/// Request shape for `move_mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MoveRequestDto {
    /// The active mods to move. They stay together, in the relative order
    /// they already have in the list.
    pub ids: Vec<String>,
    /// Where to put them.
    pub anchor: AnchorDto,
}

impl MoveRequestDto {
    /// See [`ActivateRequestDto::mod_ids`].
    #[must_use]
    pub fn mod_ids(&self) -> Vec<ModId> {
        self.ids.iter().map(ModId::new).collect()
    }
}

/// `move_mods`'s result: the selection after the move, and whether the move
/// is what changed it (the first reorder while Suggested was selected
/// switches to Current, and the page says so).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MoveModsResultDto {
    /// The selected order now.
    pub selected: OrderSourceDto,
    /// Whether this move switched the selection from Suggested to Current.
    pub did_switch_to_current: bool,
}

impl MoveModsResultDto {
    /// Pairs `outcome` with the session's selection after the move.
    #[must_use]
    pub fn new(outcome: MoveOutcome, selected: rim_resolve::domain::OrderSource) -> Self {
        Self {
            selected: selected.into(),
            did_switch_to_current: outcome.did_switch_to_current,
        }
    }
}

/// `activate_mods`'s result: the pending-change reading, plus the selection
/// after the call and whether it was this call that changed it (an anchored
/// activation switches Suggested to Current like a move; an un-anchored one
/// never does).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ActivateModsResultDto {
    /// What is still pending after the activation.
    pub pending: PendingActiveChangesDto,
    /// The selected order now.
    pub selected: OrderSourceDto,
    /// Whether this call switched the selection from Suggested to Current.
    pub did_switch_to_current: bool,
}

impl ActivateModsResultDto {
    /// Pairs the pending reading and the selection with whether `outcome`
    /// (`None` for an un-anchored activation) switched the selection.
    #[must_use]
    pub fn new(
        pending: &PendingActiveChanges,
        outcome: Option<&ActivateOutcome>,
        selected: rim_resolve::domain::OrderSource,
    ) -> Self {
        Self {
            pending: pending.into(),
            selected: selected.into(),
            did_switch_to_current: outcome.is_some_and(|outcome| outcome.did_switch_to_current),
        }
    }
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
    /// Every id that would be added to the active set, in order: each
    /// requested id's own inactive dependency closure (dependencies first),
    /// then the id itself. Where each lands is the commit's business: at the
    /// request's anchor as one block, or else at its Current slot (after any
    /// declared dependency), a mod Current never held going to the end.
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
            anchor: None,
        };

        assert_eq!(
            request.mod_ids(),
            vec![ModId::new("a.mod"), ModId::new("b.mod")]
        );
    }

    #[test]
    fn anchor_dto_round_trips_every_variant_on_the_wire() {
        let cases = [
            (
                r#"{"kind":"before","modId":"a"}"#,
                Anchor::Before(ModId::new("a")),
            ),
            (
                r#"{"kind":"after","modId":"a"}"#,
                Anchor::After(ModId::new("a")),
            ),
            (r#"{"kind":"start"}"#, Anchor::Start),
            (r#"{"kind":"end"}"#, Anchor::End),
        ];

        for (json, expected) in cases {
            let dto: AnchorDto = serde_json::from_str(json).expect("wire shape");
            assert_eq!(serde_json::to_string(&dto).expect("serializes"), json);
            assert_eq!(Anchor::from(dto), expected);
        }
    }

    #[test]
    fn move_request_dto_rejects_unknown_fields_and_a_missing_anchor() {
        let unknown = r#"{"ids":["a"],"anchor":{"kind":"start"},"extra":1}"#;
        let missing = r#"{"ids":["a"]}"#;

        assert!(serde_json::from_str::<MoveRequestDto>(unknown).is_err());
        assert!(serde_json::from_str::<MoveRequestDto>(missing).is_err());
    }

    #[test]
    fn activate_request_dto_defaults_a_missing_anchor_to_none() {
        let request: ActivateRequestDto =
            serde_json::from_str(r#"{"ids":["a"],"withDependencies":false}"#).expect("parses");

        assert_eq!(request.anchor, None);
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
