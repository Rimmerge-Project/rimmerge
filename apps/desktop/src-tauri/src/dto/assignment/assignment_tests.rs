//! Tests for the assignment DTOs.

use rim_resolve::domain::{AssignmentId, PatchModIdentity};

use super::*;
use crate::error::CommandError;
use rim_resolve::domain::{AssignmentRow, Cardinality, FieldRole, FieldSpec, RowValue, TargetRef};
use rim_session::use_cases::{CopyFromOutcome, DroppedItemSlotValue};
use std::collections::BTreeSet;

/// Pins the JSON shape the frontend relies on: each newtype map DTO
/// is the bare map, never `{"0": ...}` or a named wrapper — the
/// reason no `#[serde(transparent)]` is needed on them.
#[test]
fn newtype_map_dtos_serialize_as_the_bare_map() {
    let expected = serde_json::json!({});
    assert_eq!(
        serde_json::to_value(FieldSpecMapDto::default()).expect("serializable"),
        expected
    );
    assert_eq!(
        serde_json::to_value(TargetShapeMapDto::default()).expect("serializable"),
        expected
    );
    assert_eq!(
        serde_json::to_value(RowValueMapDto::default()).expect("serializable"),
        expected
    );
    let round_trip: FieldSpecMapDto =
        serde_json::from_value(expected).expect("a bare map deserializes");
    assert_eq!(round_trip, FieldSpecMapDto::default());
}

fn field(tag: &str) -> FieldPath {
    tag.parse().expect("valid field path")
}

#[test]
fn field_role_dto_round_trips_target_key() {
    let role = FieldRole::TargetKey {
        def_type: "ThingDef".to_string(),
    };
    let dto: FieldRoleDto = role.clone().into();
    let back: FieldRole = dto.try_into().expect("must convert back");
    assert_eq!(back, role);
}

#[test]
fn field_role_dto_round_trips_chances_field_path() {
    let role = FieldRole::Chances {
        for_slot: field("primaryTool"),
    };
    let dto: FieldRoleDto = role.clone().into();
    let back: FieldRole = dto.try_into().expect("must convert back");
    assert_eq!(back, role);
}

#[test]
fn field_role_dto_chances_rejects_a_malformed_field_path() {
    // A bare `Child` tag accepts almost any text (see `FieldPath`'s
    // own `FromStr`) — a malformed `li[...]` item spec (a non-numeric
    // `#` position) is what actually fails to parse.
    let dto = FieldRoleDto::Chances {
        for_slot: "li[#not-a-number]".to_string(),
    };
    let result: Result<FieldRole, CommandError> = dto.try_into();
    assert!(result.is_err());
}

#[test]
fn field_spec_map_dto_round_trips() {
    let mut fields = BTreeMap::new();
    fields.insert(
        field("speciesNames"),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (5, 5),
            inferred_role: None,
        },
    );
    let dto: FieldSpecMapDto = (&fields).into();
    let back: BTreeMap<FieldPath, FieldSpec> = dto.try_into().expect("must convert back");
    assert_eq!(back, fields);
}

#[test]
fn row_value_map_dto_round_trips() {
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string()]),
    );
    values.insert(
        field("chanceprimaryTool"),
        RowValue::Numbers(vec![1.0, 2.5]),
    );
    let dto: RowValueMapDto = (&values).into();
    let back: BTreeMap<FieldPath, RowValue> = dto.try_into().expect("must convert back");
    assert_eq!(back, values);
}

#[test]
fn assignment_summary_dto_counts_only_uncovered_rows_with_no_project_row() {
    use rim_resolve::domain::{Coverage, CoverageRow, DefKey};

    let identity =
        PatchModIdentity::new("mypatch.parts", "Sample Part Patch").expect("valid identity");
    let project = AssignmentProject::new(
        AssignmentId::derive(
            "profile",
            &ModId::new("mypatch.parts"),
            jiff::Timestamp::UNIX_EPOCH,
        ),
        "name".to_string(),
        identity,
        BTreeSet::new(),
        BTreeSet::new(),
        rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::from([(
                field("speciesNames"),
                FieldSpec {
                    role: FieldRole::TargetKey {
                        def_type: "ThingDef".to_string(),
                    },
                    cardinality: Cardinality::List,
                    observed: (5, 5),
                    inferred_role: None,
                },
            )]),
            target_shapes: BTreeMap::new(),
        },
        jiff::Timestamp::UNIX_EPOCH,
    );
    let target = |name: &str| TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: name.to_string(),
        },
    };
    let coverage = Coverage {
        applicable: true,
        rows: vec![
            CoverageRow {
                target: target("Elf"),
                owner: ModId::new("target.races"),
                matches: Vec::new(),
                intent: RowIntent::Cover,
                winner: None,
                has_row: false,
            },
            CoverageRow {
                target: target("Dwarf"),
                owner: ModId::new("target.races"),
                matches: Vec::new(),
                intent: RowIntent::Cover,
                winner: None,
                has_row: true,
            },
            CoverageRow {
                target: target("Orc"),
                owner: ModId::new("target.races"),
                matches: vec![rim_resolve::domain::ExistingMatch {
                    owner: ModId::new("other.mod"),
                    instance_def_name: "Group_Orc".to_string(),
                    key_field: field("speciesNames"),
                }],
                intent: RowIntent::Override,
                winner: None,
                has_row: false,
            },
        ],
    };

    let coverages = BTreeMap::from([("example.PartAssignmentDef".to_string(), coverage)]);
    let dto = assignment_summary_dto(&project, &coverages, &BTreeMap::new());

    assert_eq!(dto.uncovered_count, Some(1));
}

#[test]
fn assignment_summary_dto_uncovered_count_is_none_with_no_target_keyed_section() {
    let identity =
        PatchModIdentity::new("sample.newpart", "Sample's New Part").expect("valid identity");
    let project = AssignmentProject::new(
        AssignmentId::derive(
            "profile",
            &ModId::new("sample.newpart"),
            jiff::Timestamp::UNIX_EPOCH,
        ),
        "name".to_string(),
        identity,
        BTreeSet::new(),
        BTreeSet::new(),
        rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        },
        jiff::Timestamp::UNIX_EPOCH,
    );

    let dto = assignment_summary_dto(&project, &BTreeMap::new(), &BTreeMap::new());

    assert!(dto.is_standalone);
    assert_eq!(dto.uncovered_count, None);
}

#[test]
fn stranded_value_dto_renders_a_target_keyed_row_key_through_format_row_key() {
    let key = RowKey::Target(TargetRef {
        key_field: field("speciesNames"),
        def: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Race0".to_string(),
        },
    });

    let dto = stranded_value_dto("example.PartAssignmentDef", &key, &field("primaryTool"));

    assert_eq!(
        dto,
        StrandedValueDto {
            def_type: "example.PartAssignmentDef".to_string(),
            row: "ThingDef/Race0".to_string(),
            path: "primaryTool".to_string(),
        }
    );
}

#[test]
fn stranded_value_dto_renders_a_free_standing_row_key_as_its_bare_def_name() {
    let key = RowKey::Own("test_egg".to_string());

    let dto = stranded_value_dto("example.PartDef", &key, &field("effect"));

    assert_eq!(dto.row, "test_egg");
}

#[test]
fn dropped_item_slot_value_dto_transcribes_every_field() {
    let dropped = DroppedItemSlotValue {
        path: field("primaryTool"),
        def_type: "example.PartDef".to_string(),
        name: "Part9".to_string(),
    };

    let dto: DroppedItemSlotValueDto = (&dropped).into();

    assert_eq!(
        dto,
        DroppedItemSlotValueDto {
            path: "primaryTool".to_string(),
            def_type: "example.PartDef".to_string(),
            name: "Part9".to_string(),
        }
    );
}

#[test]
fn copy_assignment_row_result_dto_carries_both_the_row_and_the_dropped_values() {
    let outcome = CopyFromOutcome {
        row: AssignmentRow {
            values: BTreeMap::from([(
                field("primaryTool"),
                RowValue::Names(vec!["Part0".to_string()]),
            )]),
            def_name: "mypatch_parts_Race1".to_string(),
            note: None,
        },
        dropped: vec![DroppedItemSlotValue {
            path: field("primaryTool"),
            def_type: "example.PartDef".to_string(),
            name: "Part9".to_string(),
        }],
    };

    let dto: CopyAssignmentRowResultDto = (&outcome).into();

    assert_eq!(dto.row.def_name, "mypatch_parts_Race1");
    assert_eq!(dto.dropped.len(), 1);
    assert_eq!(dto.dropped[0].name, "Part9");
}
