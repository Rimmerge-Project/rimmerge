//! Tests for the assignment engine.

use super::*;
use crate::emit::defs_file_path;
use crate::tree::{FieldPath, FieldTree};
use crate::xml;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentRow, AssignmentSchema, Cardinality, FieldOccurrence, FieldRole, FieldSpec,
    InstanceValues, RowKey, RowValue, Section, TargetRef,
};
use rim_resolve::domain::{DefKey, ScalarKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

fn parse(xml_text: &str) -> FieldTree {
    xml::parse(xml_text).unwrap()
}

fn no_dll_owner(_: &str) -> Option<ModId> {
    None
}

/// `existing_def_type` for every `infer(...)` test in this module —
/// none of them exercise tag-
/// reconstruction tie discriminator, so "no type exists" keeps every
/// one classifying exactly as it did before that discriminator
/// existed.
fn no_existing_def_type(_: &str) -> bool {
    false
}

fn own_id() -> ModId {
    ModId::new("author.thisproject")
}

fn target_ref(key_field: &str, def_type: &str, def_name: &str) -> TargetRef {
    TargetRef {
        key_field: key_field.parse().unwrap(),
        def: DefKey {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
        },
    }
}

fn row(def_name: &str, values: BTreeMap<FieldPath, RowValue>) -> AssignmentRow {
    AssignmentRow {
        values,
        def_name: def_name.to_string(),
        note: None,
    }
}

// --- read_instance -----------------------------------------------

#[test]
fn read_instance_flattens_lists_and_scalars_and_a_structured_item_contributes_no_value() {
    let tree = parse(
        r#"<example.PartAssignmentDef>
                 <defName>Group_Orcs</defName>
                 <speciesNames>
                   <li>Orc</li>
                   <li>OrcWarrior</li>
                 </speciesNames>
                 <hasSingleGender>false</hasSingleGender>
                 <modExtensions>
                   <li Class="Some.Extension"><value>1</value></li>
                 </modExtensions>
               </example.PartAssignmentDef>"#,
    );

    let instance = read_instance(&tree);

    let race_names: FieldPath = "speciesNames".parse().unwrap();
    let single_gender: FieldPath = "hasSingleGender".parse().unwrap();
    let mod_extensions: FieldPath = "modExtensions".parse().unwrap();

    assert_eq!(
        instance.get(&race_names),
        Some(&FieldOccurrence {
            cardinality: Cardinality::List,
            values: vec!["Orc".to_string(), "OrcWarrior".to_string()],
        })
    );
    assert_eq!(
        instance.get(&single_gender),
        Some(&FieldOccurrence {
            cardinality: Cardinality::Scalar,
            values: vec!["false".to_string()],
        })
    );
    // A structured `li` (no text content) is present, as a list, but
    // contributes no scalar value.
    assert_eq!(
        instance.get(&mod_extensions),
        Some(&FieldOccurrence {
            cardinality: Cardinality::List,
            values: vec![],
        })
    );
}

#[test]
fn read_instance_never_records_def_name_as_a_field_b4() {
    let tree = parse(
        "<example.PartAssignmentDef><defName>Group_Orcs</defName><hasSingleGender>false</hasSingleGender></example.PartAssignmentDef>",
    );

    let instance = read_instance(&tree);

    assert!(!instance.contains_key(&"defName".parse().unwrap()));
    assert!(instance.contains_key(&"hasSingleGender".parse().unwrap()));
}

// --- `defName` never leaks into an inferred schema -------------------

#[test]
fn infer_never_classifies_def_name_as_a_field_even_when_the_names_resolve() {
    // The instances' own names ("SG_Human", ...) happen to also
    // resolve as `example.PartAssignmentDef` defNames owned by a mod inside
    // R — exactly the shape that would make a naively-recorded
    // `defName` field look like a majority-inside-R `ItemSlot`.
    let mut resolver: BTreeMap<String, Vec<(String, ModId)>> = BTreeMap::new();
    for name in ["SG_Human", "SG_Elf", "SG_Orc", "SG_Dwarf", "SG_Goblin"] {
        resolver.insert(
            name.to_string(),
            vec![(
                "example.PartAssignmentDef".to_string(),
                ModId::new("example.framework"),
            )],
        );
    }
    let resolve = |name: &str| resolver.get(name).cloned().unwrap_or_default();
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let instances: Vec<(ModId, InstanceValues)> = [
        "SG_Human",
        "SG_Elf",
        "SG_Orc",
        "SG_Dwarf",
        "SG_Goblin",
    ]
    .into_iter()
    .map(|name| {
        let xml_text = format!(
            "<example.PartAssignmentDef><defName>{name}</defName></example.PartAssignmentDef>"
        );
        (
            ModId::new("example.framework"),
            read_instance(&parse(&xml_text)),
        )
    })
    .collect();

    let schema = infer(
        "example.PartAssignmentDef",
        &instances,
        &resolve,
        &no_dll_owner,
        &no_existing_def_type,
        &refs,
    );

    assert!(!schema.fields.contains_key(&"defName".parse().unwrap()));
}

// --- infer: an end-to-end adapter smoke test ------------------------
//
// The field classification rules are `rim-resolve`'s own
// (`AssignmentSchema::infer_fields`) and tested there; this exercises
// this crate's own responsibility — turning real `FieldTree`s into
// the flat shape that function consumes — end to end.

fn race_group_instance(
    owner: &str,
    def_name: &str,
    race: &str,
    part: &str,
) -> (ModId, InstanceValues) {
    let xml_text = format!(
        r#"<example.PartAssignmentDef>
                 <defName>{def_name}</defName>
                 <speciesNames><li>{race}</li></speciesNames>
                 <primaryTool><li>{part}</li></primaryTool>
               </example.PartAssignmentDef>"#
    );
    (ModId::new(owner), read_instance(&parse(&xml_text)))
}

/// A `resolve` closure's own backing map: every `defName` this
/// fixture's resolver knows, to the `(def_type, owner)` pairs it
/// resolves to.
type Resolver = BTreeMap<String, Vec<(String, ModId)>>;

fn race_group_fixture() -> (Resolver, BTreeSet<ModId>, Vec<(ModId, InstanceValues)>) {
    let mut resolver: Resolver = BTreeMap::new();
    for (race, part) in [
        ("Human", "Wrench"),
        ("Elf", "Wrench_Elf"),
        ("Orc", "Wrench_Orc"),
        ("Dwarf", "Wrench_Dwarf"),
        ("Goblin", "Wrench_Goblin"),
    ] {
        resolver.insert(
            race.to_string(),
            vec![("ThingDef".to_string(), ModId::new("target.races"))],
        );
        resolver.insert(
            part.to_string(),
            vec![(
                "example.PartDef".to_string(),
                ModId::new("example.framework"),
            )],
        );
    }
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let instances = vec![
        race_group_instance("example.framework", "SG_Human", "Human", "Wrench"),
        race_group_instance("example.framework", "SG_Elf", "Elf", "Wrench_Elf"),
        race_group_instance("example.framework", "SG_Orc", "Orc", "Wrench_Orc"),
        race_group_instance("example.framework", "SG_Dwarf", "Dwarf", "Wrench_Dwarf"),
        race_group_instance("example.framework", "SG_Goblin", "Goblin", "Wrench_Goblin"),
    ];
    (resolver, refs, instances)
}

#[test]
fn infer_adapts_read_instance_through_assignment_schema_infer_fields() {
    let (resolver, refs, instances) = race_group_fixture();
    let resolve = |name: &str| resolver.get(name).cloned().unwrap_or_default();

    let schema = infer(
        "example.PartAssignmentDef",
        &instances,
        &resolve,
        &no_dll_owner,
        &no_existing_def_type,
        &refs,
    );

    assert_eq!(schema.def_type, "example.PartAssignmentDef");
    assert_eq!(schema.refs, refs);
    assert!(schema.target_shapes.is_empty());
    assert_eq!(
        schema.fields[&"speciesNames".parse::<FieldPath>().unwrap()].role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        }
    );
    assert_eq!(
        schema.fields[&"primaryTool".parse::<FieldPath>().unwrap()].role,
        FieldRole::ItemSlot {
            def_type: "example.PartDef".to_string()
        }
    );
}

#[test]
fn permuting_instances_yields_an_identical_schema() {
    let (resolver, refs, instances) = race_group_fixture();
    let resolve = |name: &str| resolver.get(name).cloned().unwrap_or_default();
    let mut reversed = instances.clone();
    reversed.reverse();

    let original = infer(
        "example.PartAssignmentDef",
        &instances,
        &resolve,
        &no_dll_owner,
        &no_existing_def_type,
        &refs,
    );
    let permuted = infer(
        "example.PartAssignmentDef",
        &reversed,
        &resolve,
        &no_dll_owner,
        &no_existing_def_type,
        &refs,
    );

    assert_eq!(original, permuted);
}

// --- dependencies ---------------------------------------------------

fn item_slot_schema(def_type: &str, field: &str, slot_type: &str) -> AssignmentSchema {
    AssignmentSchema {
        def_type: def_type.to_string(),
        refs: BTreeSet::new(),
        fields: BTreeMap::from([(
            field.parse().unwrap(),
            FieldSpec {
                role: FieldRole::ItemSlot {
                    def_type: slot_type.to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        )]),
        target_shapes: BTreeMap::new(),
    }
}

#[test]
fn dependencies_includes_the_framework_and_every_items_majority_owner() {
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Wrench".to_string()]),
            )]),
        ),
    );
    let resolve = |name: &str| {
        if name == "Wrench" {
            vec![(
                "example.PartDef".to_string(),
                ModId::new("example.speciessupport"),
            )]
        } else {
            Vec::new()
        }
    };
    let dll_owner =
        |t: &str| (t == "example.PartAssignmentDef").then(|| ModId::new("example.framework"));

    let deps = dependencies(&schema, &rows, &own_id(), &resolve, &dll_owner);

    assert_eq!(
        deps,
        BTreeSet::from([
            ModId::new("example.framework"),
            ModId::new("example.speciessupport")
        ])
    );
}

#[test]
fn dependencies_excludes_core_but_keeps_a_dlc_owner_s1() {
    let schema = item_slot_schema("example.PartAssignmentDef", "primaryTool", "ThingDef");
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["CoreThing".to_string(), "DlcThing".to_string()]),
            )]),
        ),
    );
    let resolve = |name: &str| match name {
        "CoreThing" => vec![("ThingDef".to_string(), ModId::new("ludeon.rimworld"))],
        "DlcThing" => vec![(
            "ThingDef".to_string(),
            ModId::new("ludeon.rimworld.biotech"),
        )],
        _ => Vec::new(),
    };

    let deps = dependencies(&schema, &rows, &own_id(), &resolve, &no_dll_owner);

    assert!(!deps.contains(&ModId::new("ludeon.rimworld")));
    assert!(deps.contains(&ModId::new("ludeon.rimworld.biotech")));
}

#[test]
fn dependencies_excludes_the_projects_own_package_id_even_as_majority_owner_6b() {
    // The resolver returns the project's own package id for "Tail" —
    // the "resolves to the own package id" case (an
    // already-exported own instance the caller's `resolve` now
    // indexes like any other def).
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Tail".to_string(), "Wrench".to_string()]),
            )]),
        ),
    );
    let own = own_id();
    let resolve = |name: &str| match name {
        "Tail" => vec![("example.PartDef".to_string(), own_id())],
        "Wrench" => vec![(
            "example.PartDef".to_string(),
            ModId::new("example.speciessupport"),
        )],
        _ => Vec::new(),
    };

    let deps = dependencies(&schema, &rows, &own, &resolve, &no_dll_owner);

    assert!(!deps.contains(&own));
    assert_eq!(deps, BTreeSet::from([ModId::new("example.speciessupport")]));
}

#[test]
fn dependencies_over_an_own_instance_that_resolves_to_nothing_adds_no_dependency_6b() {
    // The other case: an own instance the caller's `resolve`
    // never indexes at all (not yet exported) resolves to nothing —
    // same net effect (no dependency), reached via the ordinary
    // "no owner found" path rather than the exclusion above.
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Tail".to_string()]),
            )]),
        ),
    );
    let resolve = |_: &str| Vec::new();

    let deps = dependencies(&schema, &rows, &own_id(), &resolve, &no_dll_owner);

    assert!(deps.is_empty());
}

#[test]
fn dependencies_base_normalises_a_steam_suffixed_owner_against_a_bare_own_id_6b() {
    // The resolver returns the Steam-suffixed form of the own id;
    // `own_package_id` itself is the bare form — `is_own_package`
    // must still recognise them as the same mod.
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Tail".to_string()]),
            )]),
        ),
    );
    let resolve = |name: &str| {
        if name == "Tail" {
            vec![(
                "example.PartDef".to_string(),
                ModId::new("author.thisproject_steam"),
            )]
        } else {
            Vec::new()
        }
    };

    let deps = dependencies(&schema, &rows, &own_id(), &resolve, &no_dll_owner);

    assert!(deps.is_empty(), "{deps:?}");
}

#[test]
fn dependencies_base_normalises_a_bare_owner_against_a_steam_suffixed_own_id_6b() {
    // The mirror of the test above: `own_package_id` itself carries
    // the Steam suffix, the resolver returns the bare form.
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let mut rows = BTreeMap::new();
    rows.insert(
        target_ref("speciesNames", "ThingDef", "Human"),
        row(
            "Mod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Tail".to_string()]),
            )]),
        ),
    );
    let resolve = |name: &str| {
        if name == "Tail" {
            vec![(
                "example.PartDef".to_string(),
                ModId::new("author.thisproject"),
            )]
        } else {
            Vec::new()
        }
    };
    let own_steam = ModId::new("author.thisproject_steam");

    let deps = dependencies(&schema, &rows, &own_steam, &resolve, &no_dll_owner);

    assert!(deps.is_empty(), "{deps:?}");
}

// --- load_after_only ---------------------------------------------------

#[test]
fn load_after_only_excludes_core_but_keeps_a_dlc_or_mod_target() {
    let targets: BTreeSet<ModId> = [
        ModId::new("ludeon.rimworld"),
        ModId::new("ludeon.rimworld.biotech"),
        ModId::new("target.races"),
    ]
    .into_iter()
    .collect();

    let result = load_after_only(&targets);

    assert_eq!(
        result,
        BTreeSet::from([
            ModId::new("ludeon.rimworld.biotech"),
            ModId::new("target.races"),
        ])
    );
}

#[test]
fn load_after_only_base_normalizes_a_steam_suffixed_target() {
    let targets: BTreeSet<ModId> = [ModId::new("target.races_steam")].into_iter().collect();

    let result = load_after_only(&targets);

    assert_eq!(result, BTreeSet::from([ModId::new("target.races")]));
}

// --- render_rows -----------------------------------------------------

fn snapshot_schema() -> AssignmentSchema {
    AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields: BTreeMap::from([
            (
                "speciesNames".parse().unwrap(),
                FieldSpec {
                    role: FieldRole::TargetKey {
                        def_type: "ThingDef".to_string(),
                    },
                    cardinality: Cardinality::List,
                    observed: (2, 2),
                    inferred_role: None,
                },
            ),
            (
                "primaryTool".parse().unwrap(),
                FieldSpec {
                    role: FieldRole::ItemSlot {
                        def_type: "example.PartDef".to_string(),
                    },
                    cardinality: Cardinality::List,
                    observed: (2, 2),
                    inferred_role: None,
                },
            ),
            (
                "chanceprimaryTool".parse().unwrap(),
                FieldSpec {
                    role: FieldRole::Chances {
                        for_slot: "primaryTool".parse().unwrap(),
                    },
                    cardinality: Cardinality::List,
                    observed: (2, 2),
                    inferred_role: None,
                },
            ),
            (
                "hasSingleGender".parse().unwrap(),
                FieldSpec {
                    role: FieldRole::Scalar {
                        kind: ScalarKind::Bool,
                        default: Some("false".to_string()),
                    },
                    cardinality: Cardinality::Scalar,
                    observed: (1, 2),
                    inferred_role: None,
                },
            ),
            (
                "statBases/MoveSpeed".parse().unwrap(),
                FieldSpec {
                    role: FieldRole::Scalar {
                        kind: ScalarKind::Number,
                        default: None,
                    },
                    cardinality: Cardinality::Scalar,
                    observed: (1, 2),
                    inferred_role: None,
                },
            ),
        ]),
        target_shapes: BTreeMap::new(),
    }
}

fn target_section(schema: AssignmentSchema, rows: BTreeMap<TargetRef, AssignmentRow>) -> Section {
    Section {
        schema,
        rows: rows
            .into_iter()
            .map(|(target, row)| (RowKey::Target(target), row))
            .collect(),
    }
}

#[test]
fn render_rows_snapshot_with_a_core_target_and_a_modded_one() {
    let schema = snapshot_schema();
    let human_target = target_ref("speciesNames", "ThingDef", "Human");
    let elf_target = target_ref("speciesNames", "ThingDef", "Elf");
    let mut rows = BTreeMap::new();
    rows.insert(
        human_target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([
                (
                    "primaryTool".parse().unwrap(),
                    RowValue::Names(vec!["Wrench".to_string()]),
                ),
                (
                    "chanceprimaryTool".parse().unwrap(),
                    RowValue::Numbers(vec![1.0]),
                ),
                (
                    "hasSingleGender".parse().unwrap(),
                    RowValue::Text("false".to_string()),
                ),
                (
                    "statBases/MoveSpeed".parse().unwrap(),
                    RowValue::Text("13".to_string()),
                ),
            ]),
        ),
    );
    rows.insert(
        elf_target.clone(),
        row(
            "MyMod_Elf",
            BTreeMap::from([
                (
                    "primaryTool".parse().unwrap(),
                    RowValue::Names(vec!["Wrench_Elf".to_string(), "Wrench_Elf_Alt".to_string()]),
                ),
                (
                    "chanceprimaryTool".parse().unwrap(),
                    RowValue::Numbers(vec![0.7, 0.3]),
                ),
                ("hasSingleGender".parse().unwrap(), RowValue::Omit),
            ]),
        ),
    );
    let mut gates = BTreeMap::new();
    gates.insert(human_target, TargetGate::Core);
    gates.insert(elf_target, TargetGate::Mod(ModId::new("target.elves")));
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(skipped.is_empty(), "{skipped:?}");
    assert_eq!(files.len(), 1);
    let text = &files[0].content;
    assert_eq!(
        files[0].relative_path,
        defs_file_path("example.PartAssignmentDef")
    );
    insta::assert_snapshot!("render_rows_core_and_modded_target", text);

    // Elf loads first (alphabetical `def_name` sort).
    assert!(text.find("MyMod_Elf").unwrap() < text.find("MyMod_Human").unwrap());
    assert!(text.contains(r#"MayRequire="target.elves""#));
    // The Core row's own block carries no `MayRequire` at all.
    let human_block = &text[text.find("MyMod_Human").unwrap()..];
    assert!(!human_block.contains("MayRequire"));
    // The key field (`speciesNames`) is rendered from the `TargetRef`
    // itself on *both* rows, even though neither row's own `values`
    // ever set it.
    assert!(human_block.contains("<speciesNames>\n      <li>Human</li>"));
    let elf_block = &text[..text.find("MyMod_Human").unwrap()];
    assert!(elf_block.contains("<speciesNames>\n      <li>Elf</li>"));
    // The omitted field never appears in the Elf row's own block.
    assert!(!elf_block.contains("hasSingleGender"));
    assert!(human_block.contains("hasSingleGender"));
    // The nested field wraps its ancestor.
    assert!(human_block.contains("<statBases>\n      <MoveSpeed>13</MoveSpeed>"));
}

#[test]
fn target_key_field_is_always_rendered_from_the_targetref_not_the_row_b2() {
    let schema = snapshot_schema();
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    // The row's own `values` never sets `speciesNames` at all — the
    // shape `set_row` actually allows, since it neither requires nor
    // fills the key field.
    rows.insert(target.clone(), row("MyMod_Human", BTreeMap::new()));
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(skipped.is_empty(), "{skipped:?}");
    assert!(
        files[0]
            .content
            .contains("<speciesNames>\n      <li>Human</li>")
    );
}

#[test]
fn a_scalar_item_slot_with_one_name_renders_as_text_b3() {
    let mut schema = item_slot_schema("example.PartAssignmentDef", "defaultToolDef", "ThingDef");
    schema
        .fields
        .get_mut(&"defaultToolDef".parse::<FieldPath>().unwrap())
        .unwrap()
        .cardinality = Cardinality::Scalar;
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(
        target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([(
                "defaultToolDef".parse().unwrap(),
                RowValue::Names(vec!["Egg1".to_string()]),
            )]),
        ),
    );
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(skipped.is_empty(), "{skipped:?}");
    assert!(
        files[0]
            .content
            .contains("<defaultToolDef>Egg1</defaultToolDef>")
    );
    assert!(!files[0].content.contains("<li>Egg1</li>"));
}

#[test]
fn a_scalar_item_slot_with_two_names_is_skipped_not_emitted_b3() {
    let mut schema = item_slot_schema("example.PartAssignmentDef", "defaultToolDef", "ThingDef");
    schema
        .fields
        .get_mut(&"defaultToolDef".parse::<FieldPath>().unwrap())
        .unwrap()
        .cardinality = Cardinality::Scalar;
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(
        target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([(
                "defaultToolDef".parse().unwrap(),
                RowValue::Names(vec!["Egg1".to_string(), "Egg2".to_string()]),
            )]),
        ),
    );
    let mut gates = BTreeMap::new();
    gates.insert(target.clone(), TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(!files[0].content.contains("Egg1"));
    assert!(!files[0].content.contains("Egg2"));
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].row, RowKey::Target(target));
    assert_eq!(skipped[0].def_type, "example.PartAssignmentDef");
    assert_eq!(
        skipped[0].path,
        "defaultToolDef".parse::<FieldPath>().unwrap()
    );
}

#[test]
fn a_role_value_mismatch_is_skipped_with_a_reason_naming_both() {
    // Reachable via `AssignmentProject::from_stored`, which trusts a
    // stored row completely: this field's stored value was a plain
    // `Text` before the schema reclassified it `ItemSlot`.
    let schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(
        target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Text("Wrench".to_string()),
            )]),
        ),
    );
    let mut gates = BTreeMap::new();
    gates.insert(target.clone(), TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(!files[0].content.contains("Wrench"));
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].row, RowKey::Target(target));
    assert_eq!(skipped[0].path, "primaryTool".parse::<FieldPath>().unwrap());
    assert!(skipped[0].reason.contains("ItemSlot"));
    assert!(skipped[0].reason.contains("Text"));
}

#[test]
fn a_field_path_with_a_list_item_ancestor_is_skipped_not_rendered_b1() {
    // The unsafe shape: an ancestor segment (`li[#0]`), not just the
    // leaf, is a list item — never approximated by dropping it.
    let path: FieldPath = "comps/li[#0]/foo".parse().unwrap();
    let schema = AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields: BTreeMap::from([(
            path.clone(),
            FieldSpec {
                role: FieldRole::Scalar {
                    kind: ScalarKind::Text,
                    default: None,
                },
                cardinality: Cardinality::Scalar,
                observed: (1, 1),
                inferred_role: None,
            },
        )]),
        target_shapes: BTreeMap::new(),
    };
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(
        target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([(path.clone(), RowValue::Text("bar".to_string()))]),
        ),
    );
    let mut gates = BTreeMap::new();
    gates.insert(target.clone(), TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &gates);

    assert!(!files[0].content.contains("foo"));
    assert!(!files[0].content.contains("bar"));
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].row, RowKey::Target(target));
    assert_eq!(skipped[0].path, path);
    assert!(skipped[0].reason.contains("list item"));
}

#[test]
fn a_row_with_no_gate_decision_is_skipped_not_rendered_ungated_s2() {
    let schema = snapshot_schema();
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(target.clone(), row("MyMod_Human", BTreeMap::new()));
    let section = target_section(schema, rows);

    let (files, skipped) = render_rows(&section, &BTreeMap::new());

    // Every row was skipped, so there is nothing to write at all —
    // no empty `<Defs/>` shell (matching
    // `emit::render_patches_file`'s own no-blocks-no-file convention).
    assert!(files.is_empty());
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].row, RowKey::Target(target));
}

#[test]
fn render_rows_is_empty_for_no_rows() {
    let section = Section::new(snapshot_schema());
    let (files, skipped) = render_rows(&section, &BTreeMap::new());
    assert!(files.is_empty());
    assert!(skipped.is_empty());
}

#[test]
fn rendering_twice_is_byte_identical() {
    let schema = snapshot_schema();
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(target.clone(), row("MyMod_Human", BTreeMap::new()));
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);
    let section = target_section(schema, rows);

    let first = render_rows(&section, &gates);
    let second = render_rows(&section, &gates);
    assert_eq!(first, second);
}

#[test]
fn rendered_defs_file_parses_back_through_the_analyzer_defs_indexer() {
    let schema = snapshot_schema();
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut rows = BTreeMap::new();
    rows.insert(target.clone(), row("MyMod_Human", BTreeMap::new()));
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);
    let section = target_section(schema, rows);

    let (files, _) = render_rows(&section, &gates);
    let text = &files[0].content;
    let file: Arc<Path> = Arc::from(Path::new("Defs/rimmerge_example.PartAssignmentDef.xml"));

    let indexed = rim_analyzer::extract::defs::index(text.as_bytes(), &file).unwrap();
    assert_eq!(indexed.defs.len(), 1);
    assert_eq!(indexed.defs[0].def_type, "example.PartAssignmentDef");
    assert_eq!(indexed.defs[0].def_name, "MyMod_Human");
}

// --- render_rows: free-standing rows --------------------------------

fn standalone_schema() -> AssignmentSchema {
    AssignmentSchema {
        def_type: "example.PartDef".to_string(),
        refs: BTreeSet::new(),
        fields: BTreeMap::from([(
            "hediffName".parse().unwrap(),
            FieldSpec {
                role: FieldRole::Scalar {
                    kind: ScalarKind::Text,
                    default: None,
                },
                cardinality: Cardinality::Scalar,
                observed: (1, 1),
                inferred_role: None,
            },
        )]),
        target_shapes: BTreeMap::new(),
    }
}

fn standalone_row(def_name: &str, hediff_name: &str) -> AssignmentRow {
    row(
        def_name,
        BTreeMap::from([(
            "hediffName".parse().unwrap(),
            RowValue::Text(hediff_name.to_string()),
        )]),
    )
}

fn own_section(schema: AssignmentSchema, rows: BTreeMap<String, AssignmentRow>) -> Section {
    Section {
        schema,
        rows: rows
            .into_iter()
            .map(|(name, row)| (RowKey::Own(name), row))
            .collect(),
    }
}

#[test]
fn render_rows_renders_a_free_standing_row_with_no_may_require_and_no_key_field() {
    let schema = standalone_schema();
    let rows = BTreeMap::from([(
        "mypatch_newpart_Tail".to_string(),
        standalone_row("mypatch_newpart_Tail", "Tail_Hediff"),
    )]);
    let section = own_section(schema, rows);

    let (files, skipped) = render_rows(&section, &BTreeMap::new());

    assert!(skipped.is_empty(), "{skipped:?}");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, defs_file_path("example.PartDef"));
    let text = &files[0].content;
    assert!(text.contains("<defName>mypatch_newpart_Tail</defName>"));
    assert!(text.contains("<hediffName>Tail_Hediff</hediffName>"));
    assert!(
        !text.contains("MayRequire"),
        "a free-standing row has no target to gate against: {text}"
    );
}

#[test]
fn render_rows_sorts_free_standing_rows_by_def_name_and_is_byte_identical_twice() {
    let schema = standalone_schema();
    let rows = BTreeMap::from([
        (
            "sample_newpart_Zeta".to_string(),
            standalone_row("sample_newpart_Zeta", "Zeta_Hediff"),
        ),
        (
            "sample_newpart_Alpha".to_string(),
            standalone_row("sample_newpart_Alpha", "Alpha_Hediff"),
        ),
    ]);
    let section = own_section(schema, rows);

    let (first, _) = render_rows(&section, &BTreeMap::new());
    let (second, _) = render_rows(&section, &BTreeMap::new());

    assert_eq!(first, second);
    let text = &first[0].content;
    assert!(text.find("Alpha").unwrap() < text.find("Zeta").unwrap());
}

#[test]
fn render_rows_skips_a_free_standing_scalar_item_slot_with_two_names() {
    let mut schema = standalone_schema();
    schema.fields.insert(
        "part".parse().unwrap(),
        FieldSpec {
            role: FieldRole::ItemSlot {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::Scalar,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let rows = BTreeMap::from([(
        "mypatch_newpart_Tail".to_string(),
        row(
            "mypatch_newpart_Tail",
            BTreeMap::from([(
                "part".parse().unwrap(),
                RowValue::Names(vec!["A".to_string(), "B".to_string()]),
            )]),
        ),
    )]);
    let section = own_section(schema, rows);

    let (files, skipped) = render_rows(&section, &BTreeMap::new());

    assert!(!files[0].content.contains(">A<"));
    assert_eq!(skipped.len(), 1);
    assert_eq!(
        skipped[0].row,
        RowKey::Own("mypatch_newpart_Tail".to_string())
    );
    assert_eq!(skipped[0].path, "part".parse::<FieldPath>().unwrap());
}

#[test]
fn standalone_dependencies_includes_the_framework_and_item_owners() {
    let mut schema = standalone_schema();
    schema.fields.insert(
        "part".parse().unwrap(),
        FieldSpec {
            role: FieldRole::ItemSlot {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let rows = BTreeMap::from([(
        "mypatch_newpart_Tail".to_string(),
        row(
            "mypatch_newpart_Tail",
            BTreeMap::from([(
                "part".parse().unwrap(),
                RowValue::Names(vec!["Base".to_string()]),
            )]),
        ),
    )]);
    let resolve = |name: &str| {
        if name == "Base" {
            vec![("ThingDef".to_string(), ModId::new("example.speciessupport"))]
        } else {
            Vec::new()
        }
    };
    let dll_owner = |t: &str| (t == "example.PartDef").then(|| ModId::new("example.framework"));

    let deps = standalone_dependencies(&schema, &rows, &own_id(), &resolve, &dll_owner);

    assert_eq!(
        deps,
        BTreeSet::from([
            ModId::new("example.framework"),
            ModId::new("example.speciessupport")
        ])
    );
}

// --- render_sections --------------------------------------------------

#[test]
fn render_sections_renders_two_sections_into_two_files_deterministically() {
    let race_schema = snapshot_schema();
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut race_rows = BTreeMap::new();
    race_rows.insert(target.clone(), row("MyMod_Human", BTreeMap::new()));
    let race_section = target_section(race_schema, race_rows);

    let part_schema = standalone_schema();
    let part_rows = BTreeMap::from([(
        "mypatch_newpart_Tail".to_string(),
        standalone_row("mypatch_newpart_Tail", "Tail_Hediff"),
    )]);
    let part_section = own_section(part_schema, part_rows);

    let sections = BTreeMap::from([
        ("example.PartAssignmentDef".to_string(), race_section),
        ("example.PartDef".to_string(), part_section),
    ]);
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);

    let (first, first_skipped) = render_sections(&sections, &gates);
    let (second, _) = render_sections(&sections, &gates);

    assert!(first_skipped.is_empty(), "{first_skipped:?}");
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    // `BTreeMap` order: `example.PartAssignmentDef` before `example.PartDef`.
    assert_eq!(
        first[0].relative_path,
        defs_file_path("example.PartAssignmentDef")
    );
    assert_eq!(first[1].relative_path, defs_file_path("example.PartDef"));
}

#[test]
fn render_rows_handles_a_mixed_section_with_both_target_and_free_standing_rows() {
    // `Section::rows` is never actually mixed through the domain's own
    // `set_row` (a section is homogeneous by construction), but
    // `render_rows` dispatches per row on its own `RowKey` regardless
    // — this proves that dispatch is genuinely per-row, not a
    // section-wide assumption, the way a `from_stored`-trusted or
    // hand-built `Section` could in principle present one.
    let mut schema = snapshot_schema();
    // Give the schema a field usable by both rows' own values.
    schema
        .fields
        .retain(|path, _| path == &"speciesNames".parse().unwrap());
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let own_row = row("mypatch_newpart_Tail", BTreeMap::new());
    let target_row = row("MyMod_Human", BTreeMap::new());
    let rows = BTreeMap::from([
        (RowKey::Target(target.clone()), target_row),
        (RowKey::Own("mypatch_newpart_Tail".to_string()), own_row),
    ]);
    let section = Section { schema, rows };
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Mod(ModId::new("target.elves")));

    let (files, skipped) = render_rows(&section, &gates);

    assert!(skipped.is_empty(), "{skipped:?}");
    assert_eq!(files.len(), 1);
    let text = &files[0].content;
    assert!(text.contains("<defName>MyMod_Human</defName>"));
    assert!(text.contains("<defName>mypatch_newpart_Tail</defName>"));
    // Only the target-keyed row is gated — exactly one `MayRequire`
    // in the whole file, and it sits inside the target row's own
    // `<example.PartAssignmentDef ...>` opening tag, before that row's
    // `defName`.
    assert_eq!(text.matches("MayRequire").count(), 1);
    let gate = text.find(r#"MayRequire="target.elves""#).unwrap();
    let human_def_name = text.find("<defName>MyMod_Human</defName>").unwrap();
    assert!(gate < human_def_name);
}

#[test]
fn render_sections_never_treats_an_external_reference_as_dangling_even_when_a_coexisting_section_shares_its_item_type()
 {
    // Calls `render_sections` over two sections, because a single-section
    // `render_rows` call can't represent the shape a type-scoped
    // own-instance guard gets wrong — "some *other* section of the same
    // project shares this `ItemSlot`'s own referenced item type" is
    // unrepresentable in a one-`Section` signature. A free-standing
    // `example.PartDef` section (owning `mypatch_newpart_Tail`) coexists
    // with a target-keyed `example.PartAssignmentDef` row whose own
    // `primaryTool` slot names `Wrench` — a genuinely external
    // instance of the *identical* `example.PartDef` type the
    // free-standing section owns, never one of its own rows. A
    // type-scoped guard would reject exactly this (an entry exists for
    // `example.PartDef` at all, so every reference to it — not only an
    // own one — would be checked against that section's own instance
    // names alone); this engine has no such guard, so both sections
    // render with nothing skipped.
    let part_schema = standalone_schema();
    let part_rows = BTreeMap::from([(
        "mypatch_newpart_Tail".to_string(),
        standalone_row("mypatch_newpart_Tail", "Tail_Hediff"),
    )]);
    let part_section = own_section(part_schema, part_rows);

    let race_schema = item_slot_schema(
        "example.PartAssignmentDef",
        "primaryTool",
        "example.PartDef",
    );
    let target = target_ref("speciesNames", "ThingDef", "Human");
    let mut race_rows = BTreeMap::new();
    race_rows.insert(
        target.clone(),
        row(
            "MyMod_Human",
            BTreeMap::from([(
                "primaryTool".parse().unwrap(),
                RowValue::Names(vec!["Wrench".to_string()]),
            )]),
        ),
    );
    let race_section = target_section(race_schema, race_rows);

    let sections = BTreeMap::from([
        ("example.PartAssignmentDef".to_string(), race_section),
        ("example.PartDef".to_string(), part_section),
    ]);
    let mut gates = BTreeMap::new();
    gates.insert(target, TargetGate::Core);

    let (files, skipped) = render_sections(&sections, &gates);

    assert!(skipped.is_empty(), "{skipped:?}");
    let race_group_file = files
        .iter()
        .find(|f| f.relative_path == defs_file_path("example.PartAssignmentDef"))
        .expect("the PartAssignmentDef file must render");
    assert!(
        race_group_file.content.contains("Wrench"),
        "the external reference must render, not be skipped: {}",
        race_group_file.content
    );
    assert!(
        files
            .iter()
            .any(|f| f.relative_path == defs_file_path("example.PartDef")),
        "the coexisting free-standing section must still render its own file too"
    );
}
