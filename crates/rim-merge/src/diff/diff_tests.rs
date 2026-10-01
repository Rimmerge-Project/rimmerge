//! Tests for the three-way diff.

use super::*;
use crate::tree::{Content, FieldNode, FieldPath, FieldTree};
use crate::xml;
use rim_analyzer::domain::ModId;
use std::collections::{BTreeMap, BTreeSet};

fn owner(id: &str, xml_text: &str) -> OwnerVersion {
    let raw = xml::parse(xml_text).unwrap();
    OwnerVersion {
        mod_id: ModId::new(id),
        raw: raw.clone(),
        resolved: raw,
        inherited: None,
    }
}

fn mod_tree(id: &str, xml_text: &str) -> (ModId, FieldTree) {
    (ModId::new(id), xml::parse(xml_text).unwrap())
}

#[test]
fn unchanged_when_every_owner_matches_base() {
    let owners = vec![
        owner("core", "<HediffDef><label>x</label></HediffDef>"),
        owner("mod.a", "<HediffDef><label>x</label></HediffDef>"),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "label")
        .unwrap();
    assert_eq!(field.class, DiffClass::Unchanged);
}

#[test]
fn one_sided_when_exactly_one_owner_differs() {
    let owners = vec![
        owner("core", "<HediffDef><label>bionic heart</label></HediffDef>"),
        owner(
            "example.bionicsfork",
            "<HediffDef><label>synthetic heart</label></HediffDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "label")
        .unwrap();
    assert_eq!(
        field.class,
        DiffClass::OneSided {
            by: ModId::new("example.bionicsfork")
        }
    );
}

#[test]
fn agreeing_when_two_or_more_owners_share_a_different_value() {
    let owners = vec![
        owner(
            "core",
            "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
        ),
        owner(
            "flora",
            "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>",
        ),
        owner(
            "prehistoric",
            "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "plantDensity")
        .unwrap();
    assert_eq!(
        field.class,
        DiffClass::Agreeing {
            by: [ModId::new("flora"), ModId::new("prehistoric")]
                .into_iter()
                .collect()
        }
    );
}

#[test]
fn conflict_when_two_or_more_owners_disagree() {
    let owners = vec![
        owner(
            "core",
            "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
        ),
        owner(
            "flora",
            "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>",
        ),
        owner(
            "prehistoric",
            "<BiomeDef><plantDensity>1.2</plantDensity></BiomeDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "plantDensity")
        .unwrap();
    assert!(matches!(field.class, DiffClass::Conflict { .. }));
}

#[test]
fn confidence_matches_the_documented_table() {
    assert_eq!(DiffClass::Unchanged.confidence().percent(), 100);
    assert_eq!(
        DiffClass::OneSided {
            by: ModId::new("a")
        }
        .confidence()
        .percent(),
        95
    );
    assert_eq!(
        DiffClass::Agreeing {
            by: BTreeSet::new()
        }
        .confidence()
        .percent(),
        90
    );
    assert_eq!(
        DiffClass::Conflict {
            by: BTreeSet::new()
        }
        .confidence()
        .percent(),
        0
    );
}

#[test]
fn a_field_only_a_non_base_owner_has_is_absent_on_the_base_and_still_diffed() {
    let owners = vec![
        owner("core", "<HediffDef><label>x</label></HediffDef>"),
        owner(
            "example.bionicsfork",
            "<HediffDef><label>x</label><defaultLabelColor>(1,1,1)</defaultLabelColor></HediffDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "defaultLabelColor")
        .unwrap();
    assert_eq!(field.base, Value::Absent);
    assert_eq!(
        field.class,
        DiffClass::OneSided {
            by: ModId::new("example.bionicsfork")
        }
    );
}

#[test]
fn map_entry_gets_the_map_entry_kind_in_a_def_override() {
    // A direct test of `entry_kind_for`: forcing it to return
    // `EntryKind::Leaf` unconditionally must fail here.
    let owners = vec![
        owner(
            "core",
            "<BiomeDef><wildAnimals><Cobra>0.3</Cobra></wildAnimals></BiomeDef>",
        ),
        owner(
            "mod.a",
            "<BiomeDef><wildAnimals><Cobra>0.5</Cobra></wildAnimals></BiomeDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "wildAnimals/Cobra")
        .unwrap();
    assert_eq!(
        field.entry,
        EntryKind::MapEntry {
            container: "wildAnimals".parse().unwrap()
        }
    );
}

/// Regression, a real-install shape: a *flat* def — every one of its own fields a
/// distinct leaf tag, none nested, no `li` (the real
/// `ExampleAnim.HeadTypeDef/HeadNormal`'s own shape: `defName`/
/// `texPath`/`shader`/`shaderColorOverride`) — structurally
/// satisfies `ContainerKind::KeyedMap`'s own rule at its *root*
/// (non-empty, no `li`, every child tag distinct, every child
/// `Content::Text`). `map_entry_gets_the_map_entry_kind_in_a_def_override`
/// above never exercised this: its own `BiomeDef` root has a single
/// *nested* child (`wildAnimals`), so it classifies `Record`, not
/// `KeyedMap`, sidestepping the shape by construction.
/// `is_keyed_map_at` returning `false` for an empty path covers
/// `collision_fields`/`plan_patch_collision`'s own map detection, and
/// this separate call site must honour it too — otherwise every one of
/// a flat def's own top-level fields would compare as
/// `EntryKind::MapEntry` instead of `EntryKind::Leaf`.
#[test]
fn a_flat_defs_own_top_level_fields_are_leaves_not_map_entries() {
    let owners = vec![
        owner(
            "core",
            "<HeadTypeDef><defName>HeadNormal</defName><texPath>a</texPath></HeadTypeDef>",
        ),
        owner(
            "mod.a",
            "<HeadTypeDef><defName>HeadNormal</defName><texPath>b</texPath></HeadTypeDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let field = diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "texPath")
        .unwrap();
    assert_eq!(field.entry, EntryKind::Leaf, "{field:#?}");
    assert!(
        !field.is_list_item,
        "an ordinary top-level field is never a list item either: {field:#?}"
    );
}

// -- structural_change --------------

#[test]
fn a_changed_thing_class_triggers() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef><thingClass>Building</thingClass></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef><thingClass>Building_Door</thingClass></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("thingClass differs");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::ThingClass,
            by: ModId::new("mod.a"),
        }
    );
}

#[test]
fn a_changed_parent_name_triggers_even_with_no_other_field_difference() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef ParentName=\"Base1\"><defName>x</defName></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef ParentName=\"Base2\"><defName>x</defName></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("ParentName differs");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::ParentName,
            by: ModId::new("mod.a"),
        }
    );
}

#[test]
fn a_changed_root_class_attribute_triggers() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef Class=\"Vanilla.Thing\"><defName>x</defName></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef Class=\"Mod.Thing\"><defName>x</defName></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("root Class differs");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::RootClass,
            by: ModId::new("mod.a"),
        }
    );
}

#[test]
fn a_changed_class_on_an_existing_comps_li_entry_triggers() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef><comps><li Class=\"Foo\"><a>1</a></li></comps></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef><comps><li Class=\"Bar\"><a>1</a></li></comps></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("comp Class differs");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::CompClass,
            by: ModId::new("mod.a"),
        }
    );
}

#[test]
fn a_comp_only_one_owner_has_at_all_never_triggers_the_comp_class_check() {
    // An ordinary one-sided add — never "an existing entry's class
    // changed" — must not trip the positional comparison.
    let owners = vec![
        owner("core", "<ThingDef><comps></comps></ThingDef>"),
        owner(
            "mod.a",
            "<ThingDef><comps><li Class=\"Foo\"><a>1</a></li></comps></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    assert_eq!(structural_change(&diff, &owners), Ok(None));
}

#[test]
fn a_leaf_only_difference_never_triggers_the_structural_guard() {
    let owners = vec![
        owner("core", "<ThingDef><label>bionic heart</label></ThingDef>"),
        owner(
            "mod.a",
            "<ThingDef><label>Heart of Steel</label></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    assert_eq!(structural_change(&diff, &owners), Ok(None));
}

/// A middle owner that happens to match base must never be mistaken
/// for "the" differing owner — every prior test here has exactly one
/// non-base owner, which a broken implementation could pass by
/// unconditionally returning whichever owner comes first in the
/// slice, differing or not.
#[test]
fn the_correct_owner_is_named_even_when_a_middle_owner_matches_base() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef ParentName=\"Base1\"><defName>x</defName></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef ParentName=\"Base1\"><defName>x</defName></ThingDef>",
        ),
        owner(
            "mod.b",
            "<ThingDef ParentName=\"Base2\"><defName>x</defName></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("mod.b's ParentName differs from base");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::ParentName,
            by: ModId::new("mod.b"),
        },
        "mod.a matches base exactly and must never be named"
    );
}

/// Every prior single-owner test differs in exactly one field, so
/// reordering the four checks inside `structural_change` would still
/// pass them all. An owner differing in both `ParentName` and a
/// comp's `Class` must report `ParentName` — the field
/// `StructuralField` declares (and `structural_change` checks) first.
#[test]
fn a_parent_name_change_is_reported_before_a_comp_class_change_on_the_same_owner() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef ParentName=\"Base1\"><comps><li Class=\"Foo\"><a>1</a></li></comps></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef ParentName=\"Base2\"><comps><li Class=\"Bar\"><a>1</a></li></comps></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("both ParentName and the comp's Class differ");

    assert_eq!(
        change.field,
        StructuralField::ParentName,
        "ParentName is declared (and checked) before CompClass: {change:?}"
    );
}

/// The comp-class check must read each owner's `resolved` tree, not
/// its `raw` one: `mod.a` here declares no `comps` of its own at all
/// (everything would come from its `ParentName` chain) but its
/// *resolved* tree — what `inherit::resolve` actually produces —
/// carries a `comps/li` whose `Class` differs from base's. Built by
/// hand (not the `owner()` helper, which always sets `resolved:
/// raw.clone()`) so `raw` and `resolved` can genuinely disagree, the
/// way a real `ParentName`-inherited comp does.
#[test]
fn comp_class_is_compared_on_the_resolved_tree_not_the_raw_one() {
    // Both owners share the identical `ParentName` (and neither
    // declares a root `Class`), so neither of those two checks fires
    // first and masks the comp-class one under test.
    let base_raw = xml::parse(
        "<ThingDef ParentName=\"Base\"><comps><li Class=\"Foo\"><a>1</a></li></comps></ThingDef>",
    )
    .unwrap();
    let base = OwnerVersion {
        mod_id: ModId::new("core"),
        raw: base_raw.clone(),
        resolved: base_raw,
        inherited: None,
    };

    let mod_a_raw = xml::parse("<ThingDef ParentName=\"Base\"></ThingDef>").unwrap();
    let mod_a_resolved =
        xml::parse("<ThingDef><comps><li Class=\"Bar\"><a>1</a></li></comps></ThingDef>").unwrap();
    let mod_a = OwnerVersion {
        mod_id: ModId::new("mod.a"),
        raw: mod_a_raw,
        resolved: mod_a_resolved,
        inherited: None,
    };

    let owners = vec![base, mod_a];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("comp Class differs once inheritance is applied");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::CompClass,
            by: ModId::new("mod.a"),
        },
        "reading raw instead of resolved would see an empty comps \
             list for mod.a and report no change at all"
    );
}

/// Disclosed limitation, pinned per [`structural_change`]'s own doc
/// comment: removing a *non-trailing* comp shifts every later comp's
/// position by one, so a clean removal (`[Foo, Bar, Baz]` ->
/// `[Foo, Baz]`, `Bar` dropped) still compares position 1 (`Baz`,
/// shifted up) against base's own `Bar` at that position and reads as
/// an in-place class change — the guard fires the same way a genuine
/// change would, by design, not by oversight.
#[test]
fn removing_a_non_trailing_comp_is_reported_as_a_comp_class_change() {
    let owners = vec![
        owner(
            "core",
            "<ThingDef><comps><li Class=\"Foo\"><a>1</a></li>\
                 <li Class=\"Bar\"><a>2</a></li>\
                 <li Class=\"Baz\"><a>3</a></li></comps></ThingDef>",
        ),
        owner(
            "mod.a",
            "<ThingDef><comps><li Class=\"Foo\"><a>1</a></li>\
                 <li Class=\"Baz\"><a>3</a></li></comps></ThingDef>",
        ),
    ];
    let diff = three_way(&owners, &ModId::new("core")).unwrap();

    let change = structural_change(&diff, &owners)
        .unwrap()
        .expect("the shift reads as a comp class change");

    assert_eq!(
        change,
        StructuralChange {
            field: StructuralField::CompClass,
            by: ModId::new("mod.a"),
        }
    );
}

/// The same `BaseNotAnOwner` precondition `three_way` enforces —
/// checked directly here since a caller could otherwise pair a
/// `ThreeWayDiff` with a differently-filtered `owners` slice than the
/// one that actually produced it (never possible through `three_way`
/// itself, which always stamps `base` from an owner it just found in
/// the same slice, but this function takes the two independently).
#[test]
fn structural_change_reports_base_not_an_owner_the_same_way_three_way_does() {
    let owners = vec![owner("core", "<ThingDef><label>x</label></ThingDef>")];
    let diff = ThreeWayDiff {
        base: ModId::new("not.an.owner"),
        fields: Vec::new(),
    };

    let error = structural_change(&diff, &owners).unwrap_err();

    assert_eq!(error, BaseNotAnOwner(ModId::new("not.an.owner")));
}

#[test]
fn a_base_not_among_the_owners_is_an_error() {
    let owners = vec![owner("core", "<HediffDef><label>x</label></HediffDef>")];
    let error = three_way(&owners, &ModId::new("not.an.owner")).unwrap_err();
    assert_eq!(error, BaseNotAnOwner(ModId::new("not.an.owner")));
}

#[test]
fn three_way_on_a_25000_field_diff_completes_well_under_a_second_in_release() {
    fn big_owner(id: &str, offset: usize) -> OwnerVersion {
        let items: Vec<FieldNode> = (0..25_000)
            .map(|i| {
                let mut attrs = BTreeMap::new();
                attrs.insert("Class".to_string(), format!("item_{i}"));
                FieldNode {
                    tag: "li".to_string(),
                    attrs,
                    content: Content::Text((i + offset).to_string()),
                }
            })
            .collect();
        let root = FieldNode {
            tag: "ThingDef".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Children(vec![FieldNode {
                tag: "comps".to_string(),
                attrs: BTreeMap::new(),
                content: Content::Children(items),
            }]),
        };
        OwnerVersion {
            mod_id: ModId::new(id),
            raw: FieldTree {
                root: root.clone(),
                parent_name: None,
                name: None,
            },
            resolved: FieldTree {
                root,
                parent_name: None,
                name: None,
            },
            inherited: None,
        }
    }

    let owners = vec![big_owner("core", 0), big_owner("mod.a", 1)];
    let start = std::time::Instant::now();
    let diff = three_way(&owners, &ModId::new("core")).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(diff.fields.len(), 25_000);
    if cfg!(debug_assertions) {
        eprintln!("skipping the release-mode timing assertion in a debug build ({elapsed:?})");
    } else {
        assert!(
            elapsed.as_millis() < 1000,
            "three_way over 25,000 fields took {elapsed:?}, over the 1s release-mode budget"
        );
    }
}

// -- collision_fields ------

#[test]
fn collision_fields_returns_the_single_whole_subtree_field_when_not_a_map() {
    let target_raw = xml::parse("<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>").unwrap();
    let final_tree = xml::parse("<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>").unwrap();
    let per_mod = vec![mod_tree(
        "flora",
        "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>",
    )];
    let path: FieldPath = "plantDensity".parse().unwrap();

    let fields = collision_fields(&target_raw, &final_tree, &per_mod, &path);

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].path, path);
    assert_eq!(fields[0].entry, EntryKind::Leaf);
    assert!(!fields[0].is_list_item);
    assert_eq!(
        fields[0].class,
        DiffClass::OneSided {
            by: ModId::new("flora")
        }
    );
}

#[test]
fn collision_fields_expands_a_keyed_map_per_key_with_disjoint_and_conflicting_entries() {
    // A worked example, built directly (not through patch_eval::replay — a
    // real 3-mod same-key PatchOperationAdd collision literally
    // produces duplicate `<Raptor>` tags in the combined document,
    // which correctly falls back to Record/whole-subtree per
    // `container_kind`'s duplicate-tag rule; this
    // test is `collision_fields`'s own contract in isolation, given
    // trees a caller could plausibly hand it).
    let target_raw = xml::parse("<BiomeDef><wildAnimals><Tortoise>0.4</Tortoise><Cobra>0.3</Cobra></wildAnimals></BiomeDef>")
        .unwrap();
    let final_tree = xml::parse(
        "<BiomeDef><wildAnimals>\
               <Tortoise>0.4</Tortoise><Cobra>0.3</Cobra>\
               <Allosaurus>0.6</Allosaurus><Mammoth>0.1</Mammoth><Hyena>0.2</Hyena>\
               <Raptor>0.5</Raptor>\
             </wildAnimals></BiomeDef>",
    )
    .unwrap();
    let per_mod = vec![
        mod_tree(
            "a",
            "<BiomeDef><wildAnimals><Tortoise>0.4</Tortoise><Cobra>0.3</Cobra>\
                 <Allosaurus>0.6</Allosaurus><Raptor>0.3</Raptor></wildAnimals></BiomeDef>",
        ),
        mod_tree(
            "b",
            "<BiomeDef><wildAnimals><Tortoise>0.4</Tortoise><Cobra>0.3</Cobra>\
                 <Raptor>0.3</Raptor><Mammoth>0.1</Mammoth></wildAnimals></BiomeDef>",
        ),
        mod_tree(
            "c",
            "<BiomeDef><wildAnimals><Tortoise>0.4</Tortoise><Cobra>0.3</Cobra>\
                 <Raptor>0.5</Raptor><Hyena>0.2</Hyena></wildAnimals></BiomeDef>",
        ),
    ];
    let path: FieldPath = "wildAnimals".parse().unwrap();

    let fields = collision_fields(&target_raw, &final_tree, &per_mod, &path);

    let by_key: BTreeMap<String, &FieldDiff> = fields
        .iter()
        .map(|field| {
            let key = field
                .path
                .to_string()
                .trim_start_matches("wildAnimals/")
                .to_string();
            (key, field)
        })
        .collect();
    assert_eq!(
        fields
            .iter()
            .map(|field| field.path.to_string())
            .collect::<Vec<_>>(),
        vec![
            "wildAnimals/Tortoise",
            "wildAnimals/Cobra",
            "wildAnimals/Allosaurus",
            "wildAnimals/Mammoth",
            "wildAnimals/Hyena",
            "wildAnimals/Raptor",
        ],
        "union order follows final_tree's own document order"
    );
    assert_eq!(by_key["Tortoise"].class, DiffClass::Unchanged);
    assert_eq!(by_key["Cobra"].class, DiffClass::Unchanged);
    assert_eq!(
        by_key["Allosaurus"].class,
        DiffClass::OneSided {
            by: ModId::new("a")
        }
    );
    assert_eq!(
        by_key["Mammoth"].class,
        DiffClass::OneSided {
            by: ModId::new("b")
        }
    );
    assert_eq!(
        by_key["Hyena"].class,
        DiffClass::OneSided {
            by: ModId::new("c")
        }
    );
    assert_eq!(
        by_key["Raptor"].class,
        DiffClass::Conflict {
            by: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
                .into_iter()
                .collect()
        },
        "a and b agree on 0.3 but c's 0.5 disagrees, so the whole entry is a Conflict"
    );
    for field in &fields {
        assert!(matches!(field.entry, EntryKind::MapEntry { .. }));
        assert!(!field.is_list_item);
    }
}

#[test]
fn collision_fields_falls_back_to_whole_subtree_when_a_per_mod_tree_disagrees_it_is_a_map() {
    let target_raw =
        xml::parse("<BiomeDef><wildAnimals><Donkey>0.2</Donkey></wildAnimals></BiomeDef>").unwrap();
    let final_tree = xml::parse(
        "<BiomeDef><wildAnimals><Donkey>0.2</Donkey><Fox>0.1</Fox></wildAnimals></BiomeDef>",
    )
    .unwrap();
    // Mod "weird"'s own contribution left a duplicate `Donkey` tag —
    // no longer a keyed map at all, per `container_kind`'s own
    // duplicate-tag rule — so the whole container must stay a single, whole-subtree
    // comparison rather than expanding.
    let per_mod = vec![mod_tree(
        "weird",
        "<BiomeDef><wildAnimals><Donkey>0.2</Donkey><Donkey>0.9</Donkey></wildAnimals></BiomeDef>",
    )];
    let path: FieldPath = "wildAnimals".parse().unwrap();

    let fields = collision_fields(&target_raw, &final_tree, &per_mod, &path);

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].path, path);
    assert_eq!(fields[0].entry, EntryKind::Leaf);
    assert!(matches!(fields[0].base, Value::Item(_)));
}

#[test]
fn collision_fields_never_expands_a_def_root_even_when_every_child_is_a_distinct_leaf_tag() {
    // `is_keyed_map_at` must tell a def's own root (`sub_path: None`)
    // from a genuine `Dictionary<TKey,TValue>` field, though both are
    // just a node whose children are all distinct leaf tags with no `li`.
    // `whole_def_patch_collision_fixture` (`rim-session`) hits this
    // exactly: `<ThingDef><defName/><label/></ThingDef>` plus a
    // mod's own added `<techLevel/>`.
    let target_raw =
        xml::parse("<ThingDef><defName>Widget</defName><label>Widget</label></ThingDef>").unwrap();
    let final_tree = xml::parse("<ThingDef><defName>Widget</defName><label>Widget</label><techLevel>Medieval</techLevel></ThingDef>")
        .unwrap();
    let per_mod_trees = vec![(ModId::new("a.mod"), final_tree.clone())];
    let root = FieldPath::new(Vec::new());

    let fields = collision_fields(&target_raw, &final_tree, &per_mod_trees, &root);

    assert_eq!(
        fields.len(),
        1,
        "a def's own root must stay one whole-node field, never expand per top-level tag: {fields:?}"
    );
    assert_eq!(fields[0].path, root);
    assert_eq!(fields[0].entry, EntryKind::Leaf);
}

#[test]
fn collision_fields_compares_an_attributed_keyed_child_structurally_not_as_leaf_text() {
    let target_raw =
        xml::parse("<BiomeDef><wildAnimals><Donkey>0.2</Donkey></wildAnimals></BiomeDef>").unwrap();
    let final_tree = xml::parse("<BiomeDef><wildAnimals><Donkey MayRequire=\"some.mod\">0.2</Donkey></wildAnimals></BiomeDef>")
        .unwrap();
    let per_mod = vec![mod_tree(
        "gated",
        "<BiomeDef><wildAnimals><Donkey MayRequire=\"some.mod\">0.2</Donkey></wildAnimals></BiomeDef>",
    )];
    let path: FieldPath = "wildAnimals".parse().unwrap();

    let fields = collision_fields(&target_raw, &final_tree, &per_mod, &path);
    let donkey = fields
        .iter()
        .find(|field| field.path.to_string() == "wildAnimals/Donkey")
        .expect("a Donkey entry");

    // Same text (0.2) but a new attribute — never `Unchanged`/`Leaf`,
    // since that would silently drop `MayRequire` on emit.
    assert_eq!(
        donkey.class,
        DiffClass::OneSided {
            by: ModId::new("gated")
        }
    );
    assert!(
        matches!(&donkey.base, Value::Leaf(text) if text == "0.2"),
        "the base owner's own Donkey carries no attributes, so it still reads as a plain leaf"
    );
    assert!(
        matches!(&donkey.candidates[&ModId::new("gated")],
            Value::Item(node) if node.attrs.get("MayRequire") == Some(&"some.mod".to_string())
        ),
        "an attributed keyed child compares structurally, preserving MayRequire: {:?}",
        donkey.candidates[&ModId::new("gated")]
    );
}
