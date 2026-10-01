//! Tests for the finding DTOs.

use rim_analyzer::domain::{DanglingCause, EdgeKind, ModId, XmlLocator};
use rim_resolve::domain::{Confidence, FindingKey, ResolutionStatus as DomainResolutionStatus};

use super::*;
use crate::dto::common::{DefKeyDto, EdgeKindDto, ResolutionStatusDto};
use crate::dto::merge::MergeStateDto;
use crate::error::CommandError;
use rim_resolve::domain::{Action, Finding, Rationale, Suggestion};
use std::collections::BTreeMap;

#[test]
fn action_dto_round_trips_through_domain_action() {
    let action = Action::DropEdge {
        after: ModId::new("a.mod"),
        before: ModId::new("b.mod"),
        kind: EdgeKind::AssemblyRef,
    };
    let dto: ActionDto = action.clone().into();
    let back: Action = dto.try_into().expect("valid action dto");
    assert_eq!(action, back);
}

#[test]
fn merge_action_with_choices_round_trips_through_the_dto() {
    use rim_resolve::domain::{FieldPath, MergeChoice, PathSegment};

    let mut choices = BTreeMap::new();
    choices.insert(
        FieldPath::new(vec![PathSegment::Child("label".to_string())]),
        MergeChoice::From {
            mod_id: ModId::new("a.mod"),
        },
    );
    let action = Action::Merge {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        choices,
    };

    let dto: ActionDto = action.clone().into();
    let back: Action = dto.try_into().expect("valid action dto");
    assert_eq!(action, back);
}

#[test]
fn merge_action_dto_serializes_choices_keyed_by_the_field_paths_text_form() {
    let mut choices = BTreeMap::new();
    choices.insert(
        "label".to_string(),
        MergeChoiceDto::From {
            mod_id: "a.mod".to_string(),
        },
    );
    let dto = ActionDto::Merge {
        key: DefKeyDto {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        choices,
    };

    let json = serde_json::to_value(&dto).expect("serializable");
    assert_eq!(json["kind"], "merge");
    assert_eq!(json["choices"]["label"]["choice"], "from");
    assert_eq!(json["choices"]["label"]["modId"], "a.mod");
}

#[test]
fn merge_action_dto_rejects_an_unparseable_field_path() {
    let dto = ActionDto::Merge {
        key: DefKeyDto {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        choices: BTreeMap::from([("li[unterminated".to_string(), MergeChoiceDto::Drop)]),
    };

    let result: Result<Action, CommandError> = dto.try_into();
    assert!(result.is_err());
}

#[test]
fn ship_asset_action_round_trips_through_the_dto() {
    let action = Action::ShipAsset {
        texture_path: "Things/Wall.png".to_string(),
        from: ModId::new("a.mod"),
    };
    let dto: ActionDto = action.clone().into();
    let json = serde_json::to_value(&dto).expect("serializable");
    assert_eq!(json["kind"], "shipAsset");
    assert_eq!(json["texturePath"], "Things/Wall.png");

    let back: Action = dto.try_into().expect("valid action dto");
    assert_eq!(action, back);
}

#[test]
fn action_dto_serializes_struct_variant_fields_as_camel_case() {
    let dto = ActionDto::DropEdge {
        after: "a.mod".to_string(),
        before: "b.mod".to_string(),
        edge_kind: EdgeKindDto::AssemblyRef,
    };
    let json = serde_json::to_value(&dto).expect("serializable");
    assert_eq!(json["kind"], "dropEdge");
    assert!(
        json.get("edgeKind").is_some(),
        "struct-variant fields must be camelCase like every other DTO, got {json}"
    );
}

#[test]
fn action_dto_rejects_an_invalid_tag_slug() {
    let dto = ActionDto::AddTag {
        mod_id: "a.mod".to_string(),
        tag: "Not Valid".to_string(),
    };
    let result: Result<Action, CommandError> = dto.try_into();
    assert!(result.is_err());
}

#[test]
fn finding_dto_maps_edge_dropped() {
    let finding = Finding::EdgeDropped {
        after: ModId::new("a.mod"),
        before: ModId::new("b.mod"),
        kind: EdgeKind::LoadAfter,
        detail: "loadAfter".to_string(),
        strength: rim_analyzer::domain::EdgeStrength::Declared,
        winner: None,
    };
    let dto: FindingDto = (&finding).into();
    match dto {
        FindingDto::EdgeDropped { after, before, .. } => {
            assert_eq!(after, "a.mod");
            assert_eq!(before, "b.mod");
        }
        other => panic!("expected EdgeDropped, got {other:?}"),
    }
}

#[test]
fn texture_override_dto_sends_the_selected_order_winner_beside_the_scan_order_owners() {
    // The winner is the first owner, so it cannot be read off `owners.last()`.
    let finding = Finding::TextureOverride {
        texture_path: "Things/Wall".to_string(),
        owners: vec![ModId::new("a.mod"), ModId::new("b.mod")],
        winner: ModId::new("a.mod"),
    };

    let json = serde_json::to_value(FindingDto::from(&finding)).expect("serializes");

    assert_eq!(json["kind"], "textureOverride");
    assert_eq!(json["owners"], serde_json::json!(["a.mod", "b.mod"]));
    assert_eq!(json["winner"], "a.mod");
}

#[test]
fn resolution_status_dto_maps_every_domain_variant() {
    assert_eq!(
        ResolutionStatusDto::from(DomainResolutionStatus::Auto),
        ResolutionStatusDto::Auto
    );
    assert_eq!(
        ResolutionStatusDto::from(DomainResolutionStatus::NeedsInput),
        ResolutionStatusDto::NeedsInput
    );
    assert_eq!(
        ResolutionStatusDto::from(DomainResolutionStatus::UserOverridden),
        ResolutionStatusDto::UserOverridden
    );
}

#[test]
fn suggestion_dto_carries_confidence_as_a_plain_percent() {
    let suggestion = Suggestion {
        action: Action::Accept,
        confidence: Confidence::new(72).expect("valid confidence"),
        rationale: Rationale::MissingModNotInstalled,
        alternatives: Vec::new(),
    };
    let dto: SuggestionDto = (&suggestion).into();
    assert_eq!(dto.confidence, 72);
}

fn resolution_with_merge_state(
    merge: Option<rim_resolve::domain::MergeState>,
) -> rim_resolve::domain::Resolution {
    resolution_with_merge_state_and_guard(merge, None)
}

fn resolution_with_merge_state_and_guard(
    merge: Option<rim_resolve::domain::MergeState>,
    structural_guard_field: Option<&str>,
) -> rim_resolve::domain::Resolution {
    rim_resolve::domain::Resolution {
        key: FindingKey::MissingMod {
            mod_id: ModId::new("a.mod"),
        },
        finding: Finding::MissingMod {
            mod_id: ModId::new("a.mod"),
        },
        suggestion: Suggestion {
            action: Action::Accept,
            confidence: Confidence::new(60).expect("valid confidence"),
            rationale: Rationale::MissingModNotInstalled,
            alternatives: Vec::new(),
        },
        status: DomainResolutionStatus::NeedsInput,
        effective: Action::Accept,
        decision: None,
        resolved_by_suggested: None,
        merge,
        structural_guard_field: structural_guard_field.map(str::to_string),
        scope: None,
    }
}

#[test]
fn resolution_summary_dto_carries_the_merge_state_through() {
    let with_merge = resolution_with_merge_state(Some(rim_resolve::domain::MergeState::Complete {
        op_count: 2,
    }));
    let dto: ResolutionSummaryDto = (&with_merge).into();
    assert!(matches!(
        dto.merge_state,
        Some(MergeStateDto::Complete { op_count: 2 })
    ));

    let without_merge = resolution_with_merge_state(None);
    let dto: ResolutionSummaryDto = (&without_merge).into();
    assert_eq!(dto.merge_state, None);
}

#[test]
fn resolution_detail_dto_carries_the_merge_state_through() {
    let with_merge =
        resolution_with_merge_state(Some(rim_resolve::domain::MergeState::NeedsFieldInput {
            unresolved: 1,
            total: 4,
        }));
    let dto: ResolutionDetailDto = (&with_merge).into();
    assert!(matches!(
        dto.merge_state,
        Some(MergeStateDto::NeedsFieldInput {
            unresolved: 1,
            total: 4
        })
    ));

    let without_merge = resolution_with_merge_state(None);
    let dto: ResolutionDetailDto = (&without_merge).into();
    assert_eq!(dto.merge_state, None);
}

/// A pre-existing `Merge` decision on a def that has since become
/// guarded (`rim-session`'s `apply_merge_states` populates both
/// `merge`/`structural_guard_field` together in that case — see
/// `crates/rim-session/CLAUDE.md`) must carry the guard field through
/// to both DTOs, so `FindingCard.vue`/`MergeModEntryList.vue` can
/// render "confirm the winner" instead of "N fields need input".
#[test]
fn resolution_summary_and_detail_dto_carry_the_structural_guard_field_through() {
    let guarded = resolution_with_merge_state_and_guard(
        Some(rim_resolve::domain::MergeState::NeedsFieldInput {
            unresolved: 9,
            total: 9,
        }),
        Some("thingClass"),
    );
    let summary: ResolutionSummaryDto = (&guarded).into();
    assert_eq!(
        summary.structural_guard_field.as_deref(),
        Some("thingClass")
    );
    let detail: ResolutionDetailDto = (&guarded).into();
    assert_eq!(detail.structural_guard_field.as_deref(), Some("thingClass"));

    let unguarded =
        resolution_with_merge_state(Some(rim_resolve::domain::MergeState::NeedsFieldInput {
            unresolved: 1,
            total: 4,
        }));
    let summary: ResolutionSummaryDto = (&unguarded).into();
    assert_eq!(summary.structural_guard_field, None);
}

#[test]
fn rationale_dto_edge_dropped_with_winner_carries_every_field() {
    let rationale = Rationale::EdgeDroppedWithWinner {
        after: ModId::new("a.mod"),
        before: ModId::new("b.mod"),
        winner: rim_resolve::domain::EdgeWinner {
            after: ModId::new("a.mod"),
            before: ModId::new("c.mod"),
            layer: rim_resolve::sort::Layer::Hard,
            detail: "a hard requirement".to_string(),
        },
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::EdgeDroppedWithWinner {
            after: "a.mod".to_string(),
            before: "b.mod".to_string(),
            winner: EdgeWinnerDto {
                after: "a.mod".to_string(),
                before: "c.mod".to_string(),
                layer: crate::dto::order::LayerDto::Hard,
                detail: "a hard requirement".to_string(),
            },
        }
    );
}

#[test]
fn rationale_dto_placement_promotes_dependents_maps_every_promoted_mod() {
    let rationale = Rationale::PlacementPromotesDependents {
        mod_id: ModId::new("a.mod"),
        placement: rim_resolve::domain::Placement::Top,
        promoted: vec![ModId::new("b.mod"), ModId::new("c.mod")],
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::PlacementPromotesDependents {
            mod_id: "a.mod".to_string(),
            placement: crate::dto::common::PlacementDto::Top,
            promoted: vec!["b.mod".to_string(), "c.mod".to_string()],
        }
    );
}

#[test]
fn rationale_dto_rule_overruled_longer_cycle_carries_the_origin_and_override_flag() {
    let rationale = Rationale::RuleOverruledLongerCycle {
        after: ModId::new("a.mod"),
        before: ModId::new("b.mod"),
        origin: rim_resolve::domain::RuleOrigin::RimSortUser,
        overrides_declared: true,
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::RuleOverruledLongerCycle {
            after: "a.mod".to_string(),
            before: "b.mod".to_string(),
            origin: crate::dto::common::RuleOriginDto::RimSortUser,
            overrides_declared: true,
        }
    );
}

#[test]
fn rationale_dto_edge_dropped_inferred_carries_the_edge_kind_not_the_serde_tag() {
    let rationale = Rationale::EdgeDroppedInferred {
        kind: EdgeKind::PatchRemovedNode,
        after: ModId::new("a.mod"),
        before: ModId::new("b.mod"),
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::EdgeDroppedInferred {
            edge_kind: EdgeKindDto::PatchRemovedNode,
            after: "a.mod".to_string(),
            before: "b.mod".to_string(),
        }
    );
}

#[test]
fn rationale_dto_tag_inferred_signal_count_carries_the_count() {
    let dto: RationaleDto = (&Rationale::TagInferredSignalCount { count: 3 }).into();
    assert_eq!(dto, RationaleDto::TagInferredSignalCount { count: 3 });
}

#[test]
fn rationale_dto_a_unit_variant_maps_with_no_fields() {
    let dto: RationaleDto = (&Rationale::KeepEdgeInCycle).into();
    assert_eq!(dto, RationaleDto::KeepEdgeInCycle);
}

#[test]
fn rationale_dto_dangling_def_reference_carries_the_cause_and_sound_flag() {
    let rationale = Rationale::DanglingDefReference {
        cause: DanglingCause::RemovedBy {
            mod_id: ModId::new("a.mod"),
            locator: XmlLocator::new(std::sync::Arc::from(std::path::Path::new("a.xml")), vec![0]),
        },
        likely_sound: true,
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::DanglingDefReference {
            cause: DanglingCauseDto::RemovedBy {
                mod_id: "a.mod".to_string(),
                file: Some("a.xml".to_string()),
            },
            likely_sound: true,
        }
    );
}

#[test]
fn rationale_dto_patch_will_fail_removed_by_carries_every_field() {
    let rationale = Rationale::PatchWillFailRemovedBy {
        remover: ModId::new("a.mod"),
        mod_id: ModId::new("b.mod"),
        xpath: "Defs/ThingDef[defName=\"Wall\"]/comps".to_string(),
    };
    let dto: RationaleDto = (&rationale).into();
    assert_eq!(
        dto,
        RationaleDto::PatchWillFailRemovedBy {
            remover: "a.mod".to_string(),
            mod_id: "b.mod".to_string(),
            xpath: "Defs/ThingDef[defName=\"Wall\"]/comps".to_string(),
        }
    );
}

#[test]
fn rationale_dto_duplicate_template_name_explanation_carries_the_owner_count() {
    let dto: RationaleDto =
        (&Rationale::DuplicateTemplateNameExplanation { owner_count: 3 }).into();
    assert_eq!(
        dto,
        RationaleDto::DuplicateTemplateNameExplanation { owner_count: 3 }
    );
}

#[test]
fn rationale_dto_a_defs_unit_variant_maps_with_no_fields() {
    let dto: RationaleDto = (&Rationale::DefOverrideSameAuthor).into();
    assert_eq!(dto, RationaleDto::DefOverrideSameAuthor);
}

#[test]
fn suggestion_dto_carries_both_the_english_rationale_and_its_structured_code() {
    let suggestion = Suggestion {
        action: Action::Accept,
        confidence: Confidence::new(72).expect("valid confidence"),
        rationale: Rationale::RejectInferredTag,
        alternatives: Vec::new(),
    };
    let dto: SuggestionDto = (&suggestion).into();
    assert_eq!(dto.rationale, "Reject the inferred tag.");
    assert_eq!(dto.rationale_code, RationaleDto::RejectInferredTag);
}
