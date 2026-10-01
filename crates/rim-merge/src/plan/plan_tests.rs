//! Tests for merge planning.

use super::fields::{DropOutcome, plan_drop};
use super::rendering::build_chain;
use super::*;
use crate::diff::DiffClass;
use crate::inherit::TemplateSet;
use crate::tree::Content;
use crate::xml;

#[test]
fn caveat_display_names_every_out_of_scope_owner() {
    let caveat = Caveat::OutOfScopeOwners {
        mods: vec![ModId::new("mod.a"), ModId::new("mod.b")],
    };
    let text = caveat.to_string();
    assert!(text.contains("mod.a, mod.b"), "{text}");
    assert!(text.contains("loads after this patch's scope"), "{text}");
}

#[test]
fn caveat_display_names_the_failed_ops_mod_and_xpath() {
    let caveat = Caveat::FailedOp {
        mod_id: ModId::new("mod.a"),
        xpath: "Defs/ThingDef[defName=\"Wall\"]/label".to_string(),
    };
    assert_eq!(
        caveat.to_string(),
        "mod.a's patch operation on Defs/ThingDef[defName=\"Wall\"]/label matched nothing"
    );
}

fn owner(id: &str, xml_text: &str) -> OwnerVersion {
    let raw = xml::parse(xml_text).unwrap();
    OwnerVersion {
        mod_id: ModId::new(id),
        raw: raw.clone(),
        resolved: raw,
        inherited: None,
    }
}

/// Builds an owner whose `resolved`/`inherited` trees are driven
/// through [`crate::inherit::resolve`]/[`crate::inherit::resolve_inherited_only`]
/// against a real [`TemplateSet`], rather than hand-built, so the rule-4
/// tests exercise the real "also inherited" check.
fn owner_via_templates(id: &str, raw_xml: &str, templates: &TemplateSet) -> OwnerVersion {
    let raw = xml::parse(raw_xml).unwrap();
    let resolved = crate::inherit::resolve(&raw, templates).unwrap();
    let inherited = crate::inherit::resolve_inherited_only(&raw, templates).unwrap();
    OwnerVersion {
        mod_id: ModId::new(id),
        raw,
        resolved,
        inherited,
    }
}

fn template_set(entries: Vec<(&str, &str, &str)>) -> TemplateSet {
    let mut by_name = BTreeMap::new();
    for (def_type, name, xml_text) in entries {
        let tree = xml::parse(xml_text).unwrap();
        by_name.insert((def_type.to_string(), name.to_string()), tree);
    }
    TemplateSet::new(by_name)
}

fn choices(pairs: Vec<(&str, MergeChoice)>) -> BTreeMap<FieldPath, MergeChoice> {
    pairs
        .into_iter()
        .map(|(path, choice)| (path.parse().unwrap(), choice))
        .collect()
}

#[test]
fn a_merge_with_no_choices_over_two_owners_is_a_no_op() {
    // With two owners the base is the earlier one, so every OneSided
    // result already equals the winner's own resolved value.
    let core = owner(
        "ludeon.rimworld",
        "<HediffDef><label>bionic heart</label></HediffDef>",
    );
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>synthetic heart</label></HediffDef>",
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();

    let plan = plan_def_override(&diff, &bionics, &BTreeMap::new());

    assert!(plan.ops.is_empty());
    assert!(plan.unresolved.is_empty());
}

#[test]
fn rule_1_replace_when_the_field_exists_in_the_raw_winner() {
    let core = owner(
        "ludeon.rimworld",
        "<HediffDef><label>bionic heart</label></HediffDef>",
    );
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>synthetic heart</label></HediffDef>",
    );
    let diff = crate::diff::three_way(
        &[core.clone(), bionics.clone()],
        &ModId::new("ludeon.rimworld"),
    )
    .unwrap();
    let chosen = choices(vec![(
        "label",
        MergeChoice::From {
            mod_id: ModId::new("ludeon.rimworld"),
        },
    )]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::Replace { path, node } => {
            assert_eq!(path.to_string(), "label");
            assert_eq!(node.content, Content::Text("bionic heart".to_string()));
        }
        other => panic!("expected Replace, got {other:?}"),
    }
    assert_eq!(
        plan.ops[0].depends_on,
        [ModId::new("example.bionicsfork")].into_iter().collect()
    );
    assert_eq!(
        plan.owners,
        vec![
            ModId::new("example.bionicsfork"),
            ModId::new("ludeon.rimworld")
        ]
    );
}

#[test]
fn rule_3_add_when_the_field_is_missing_from_the_raw_winner() {
    // `defaultLabelColor` is inherited on BIONICS's raw node (comes from
    // the `addedPartExampleSynth` template), matching the Bionics worked
    // example.
    let templates = template_set(vec![(
        "HediffDef",
        "addedPartExampleSynth",
        "<HediffDef Name=\"addedPartExampleSynth\" Abstract=\"True\"><defaultLabelColor>(188,39,242)</defaultLabelColor></HediffDef>",
    )]);
    let core = owner(
        "ludeon.rimworld",
        "<HediffDef><defaultLabelColor>(0.6, 0.6, 1.0)</defaultLabelColor></HediffDef>",
    );
    let bionics = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="addedPartExampleSynth"><label>synthetic heart</label></HediffDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![(
        "defaultLabelColor",
        MergeChoice::From {
            mod_id: ModId::new("ludeon.rimworld"),
        },
    )]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::Add { parent, node } => {
            assert_eq!(parent.to_string(), "");
            assert_eq!(node.tag, "defaultLabelColor");
            assert_eq!(node.content, Content::Text("(0.6, 0.6, 1.0)".to_string()));
        }
        other => panic!("expected Add, got {other:?}"),
    }
}

#[test]
fn rule_4_remove_when_the_dropped_item_exists_only_in_the_raw_winner() {
    let core = owner("ludeon.rimworld", "<HediffDef></HediffDef>");
    let bionics = owner(
        "example.bionicsfork",
        r#"<HediffDef><comps><li Class="Foo"><x>1</x></li></comps></HediffDef>"#,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![("comps/li[@Class=Foo]", MergeChoice::Drop)]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 1);
    assert!(
        matches!(&plan.ops[0].op, PlanOp::Remove { path } if path.to_string() == "comps/li[@Class=Foo]")
    );
}

#[test]
fn rule_4_replace_inherit_false_when_the_field_is_defined_in_both_raw_and_the_ancestor_chain() {
    // Regression: `props/mode` is a *named* (not `li`-list)
    // nested field both BIONICS's raw node and its template define —
    // `merge_over` recurses into same-named elements (never appends
    // duplicates the way `li` does), so `winner.resolved` shows only
    // raw's own value here, and a plain `Remove` of raw's own `mode`
    // would leave the template's own `mode` to resurface through
    // inheritance once our patch's `Remove` no longer masks it. (A
    // `li`-identified item can't reproduce this scenario: two `li`s
    // sharing one identity are never merged into one node at all —
    // `merge_children` always appends both, which `identify_all_li`'s
    // own duplicate-collision fallback then tells apart by position,
    // making each independently a plain `Remove` case instead.)
    let templates = template_set(vec![(
        "ThingDef",
        "Base",
        r#"<ThingDef Name="Base" Abstract="True"><props><mode>from-template</mode></props></ThingDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<ThingDef></ThingDef>");
    let mod_a = owner_via_templates(
        "mod.a",
        r#"<ThingDef ParentName="Base"><props><mode>from-raw</mode></props></ThingDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, mod_a.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![("props/mode", MergeChoice::Drop)]);

    let plan = plan_def_override(&diff, &mod_a, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::ReplaceInheritFalse { path, node } => {
            assert_eq!(path.to_string(), "props");
            assert_eq!(node.attrs.get("Inherit").map(String::as_str), Some("False"));
            let Content::Children(items) = &node.content else {
                unreachable!()
            };
            assert!(
                items.is_empty(),
                "the reconstructed props must not carry mode from either source"
            );
        }
        other => panic!("expected ReplaceInheritFalse, got {other:?}"),
    }
}

#[test]
fn rule_4_replace_inherit_false_when_the_container_exists_but_the_item_does_not() {
    let templates = template_set(vec![(
        "HediffDef",
        "Base",
        r#"<HediffDef Name="Base" Abstract="True"><comps><li Class="Inherited"/></comps></HediffDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<HediffDef></HediffDef>");
    let bionics = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="Base"><comps><li Class="Own"/></comps></HediffDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![("comps/li[@Class=Inherited]", MergeChoice::Drop)]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::ReplaceInheritFalse { path, node } => {
            assert_eq!(path.to_string(), "comps");
            assert_eq!(node.attrs.get("Inherit").map(String::as_str), Some("False"));
            let Content::Children(items) = &node.content else {
                unreachable!()
            };
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].attrs.get("Class").map(String::as_str), Some("Own"));
        }
        other => panic!("expected ReplaceInheritFalse, got {other:?}"),
    }
}

#[test]
fn rule_4_add_inherit_false_when_the_container_is_absent_from_the_raw_winner() {
    // The Bionics worked example: BIONICS's raw node has no `comps`
    // at all (it's purely inherited from `addedPartExampleSynth`).
    let templates = template_set(vec![(
        "HediffDef",
        "addedPartExampleSynth",
        r#"<HediffDef Name="addedPartExampleSynth" Abstract="True"><comps><li Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust"><scaleAdjustment>0.20</scaleAdjustment></li></comps></HediffDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<HediffDef></HediffDef>");
    let bionics = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="addedPartExampleSynth"><label>synthetic heart</label></HediffDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![(
        "comps/li[@Class=ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust]",
        MergeChoice::Drop,
    )]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::Add { parent, node } => {
            assert_eq!(parent.to_string(), "");
            assert_eq!(node.tag, "comps");
            assert_eq!(node.attrs.get("Inherit").map(String::as_str), Some("False"));
            let Content::Children(items) = &node.content else {
                unreachable!()
            };
            assert!(items.is_empty());
        }
        other => panic!("expected Add, got {other:?}"),
    }
}

#[test]
fn rule_4_dropping_a_purely_inherited_top_level_leaf_is_unresolved_not_re_added() {
    // Regression: `label` is a one-segment path, purely inherited
    // — there's no container to reconstruct minus itself.
    let templates = template_set(vec![(
        "HediffDef",
        "Base",
        r#"<HediffDef Name="Base" Abstract="True"><label>from template</label></HediffDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<HediffDef></HediffDef>");
    let bionics = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="Base"><description>d</description></HediffDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![("label", MergeChoice::Drop)]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert!(plan.ops.is_empty());
    assert_eq!(plan.unresolved, vec!["label".parse::<FieldPath>().unwrap()]);
    assert!(matches!(&plan.caveats[0], Caveat::UnsettableLeaf { .. }));
}

#[test]
fn rule_4_dropping_a_nested_purely_inherited_leaf_still_reconstructs_the_container() {
    // The `props/b` shape: `b` is nested (not top-level), so the
    // container (`props`) *can* be reconstructed minus `b`.
    let templates = template_set(vec![(
        "ThingDef",
        "Base",
        r#"<ThingDef Name="Base" Abstract="True"><props><a>1</a><b>2</b></props></ThingDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<ThingDef></ThingDef>");
    let mod_a = owner_via_templates(
        "mod.a",
        r#"<ThingDef ParentName="Base"><label>x</label></ThingDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, mod_a.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![("props/b", MergeChoice::Drop)]);

    let plan = plan_def_override(&diff, &mod_a, &chosen);

    assert_eq!(plan.ops.len(), 1);
    match &plan.ops[0].op {
        PlanOp::Add { parent, node } => {
            assert_eq!(parent.to_string(), "");
            assert_eq!(node.tag, "props");
            assert_eq!(node.attrs.get("Inherit").map(String::as_str), Some("False"));
            let Content::Children(items) = &node.content else {
                unreachable!()
            };
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].tag, "a");
        }
        other => panic!("expected Add, got {other:?}"),
    }
}

#[test]
fn depends_on_never_includes_core_but_always_includes_the_winner_and_from_source() {
    let core = owner("ludeon.rimworld", "<HediffDef><label>x</label></HediffDef>");
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>y</label></HediffDef>",
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![(
        "label",
        MergeChoice::From {
            mod_id: ModId::new("ludeon.rimworld"),
        },
    )]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    let depends_on = &plan.ops[0].depends_on;
    assert!(!depends_on.contains(&ModId::new("ludeon.rimworld")));
    assert!(depends_on.contains(&ModId::new("example.bionicsfork")));
}

#[test]
fn a_conflict_field_with_no_stored_choice_is_unresolved() {
    let core = owner(
        "core",
        "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
    );
    let a = owner("a", "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>");
    let b = owner("b", "<BiomeDef><plantDensity>1.2</plantDensity></BiomeDef>");
    let diff = crate::diff::three_way(&[core, a.clone(), b], &ModId::new("core")).unwrap();

    let plan = plan_def_override(&diff, &a, &BTreeMap::new());

    assert_eq!(
        plan.unresolved,
        vec!["plantDensity".parse::<FieldPath>().unwrap()]
    );
    assert!(plan.ops.is_empty());
}

#[test]
fn an_unknown_owner_choice_is_unresolved_with_a_caveat() {
    let core = owner("ludeon.rimworld", "<HediffDef><label>x</label></HediffDef>");
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>y</label></HediffDef>",
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![(
        "label",
        MergeChoice::From {
            mod_id: ModId::new("not.an.owner"),
        },
    )]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.unresolved, vec!["label".parse::<FieldPath>().unwrap()]);
    assert!(matches!(
        &plan.caveats[0],
        Caveat::UnknownOwnerChoice { .. }
    ));
}

#[test]
fn an_invalid_value_fragment_is_unresolved_with_a_caveat_never_a_silent_drop() {
    let core = owner(
        "ludeon.rimworld",
        r#"<HediffDef><comps><li Class="A"/></comps></HediffDef>"#,
    );
    let mod_a = owner(
        "mod.a",
        r#"<HediffDef><comps><li Class="B"/></comps></HediffDef>"#,
    );
    let diff =
        crate::diff::three_way(&[core, mod_a.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    // Two `<li>` roots — not a single item, so this must not parse.
    let chosen = choices(vec![(
        "comps/li[@Class=B]",
        MergeChoice::Value {
            text: "<li Class=\"X\"/><li Class=\"Y\"/>".to_string(),
        },
    )]);

    let plan = plan_def_override(&diff, &mod_a, &chosen);

    assert!(plan.ops.is_empty());
    assert_eq!(
        plan.unresolved,
        vec!["comps/li[@Class=B]".parse::<FieldPath>().unwrap()]
    );
    assert!(matches!(
        &plan.caveats[0],
        Caveat::InvalidValueFragment { .. }
    ));
}

#[test]
fn a_value_containing_both_quote_characters_is_unresolved_with_a_caveat() {
    let templates = template_set(vec![(
        "ThingDef",
        "Base",
        r#"<ThingDef Name="Base" Abstract="True"><comps><li Class="both&quot;'kinds"><x>1</x></li></comps></ThingDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<ThingDef></ThingDef>");
    let mod_a = owner_via_templates(
        "mod.a",
        r#"<ThingDef ParentName="Base"><label>x</label></ThingDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, mod_a.clone()], &ModId::new("ludeon.rimworld")).unwrap();

    let plan = plan_def_override(&diff, &mod_a, &BTreeMap::new());

    let unsafe_path: FieldPath = "comps/li[@Class=both\\\"'kinds]".parse().unwrap();
    assert!(plan.unresolved.contains(&unsafe_path));
    assert!(
        plan.caveats
            .iter()
            .any(|c| matches!(c, Caveat::UnsafeXpathValue { path } if path == &unsafe_path))
    );
}

#[test]
fn a_value_containing_a_double_dot_or_pipe_is_unresolved_with_a_caveat() {
    // Neither character is a quoting hazard by itself, but
    // `rim_analyzer::extract::xpath_expr`'s own unsupported-xpath
    // scan for `..`/`|` is a naive substring check over the *whole*
    // remaining xpath, not bracket/quote-depth-aware — so a value
    // containing either would make the emitted xpath parse as
    // `Unsupported` even though it sits safely inside quotes. Not
    // this crate's parser to fix; must be refused at plan time
    // instead.
    let templates = template_set(vec![(
        "ThingDef",
        "Base",
        r#"<ThingDef Name="Base" Abstract="True"><comps><li Class="has..dots"><x>1</x></li><li Class="has|pipe"><x>2</x></li></comps></ThingDef>"#,
    )]);
    let core = owner("ludeon.rimworld", "<ThingDef></ThingDef>");
    let mod_a = owner_via_templates(
        "mod.a",
        r#"<ThingDef ParentName="Base"><label>x</label></ThingDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, mod_a.clone()], &ModId::new("ludeon.rimworld")).unwrap();

    let plan = plan_def_override(&diff, &mod_a, &BTreeMap::new());

    let dots_path: FieldPath = "comps/li[@Class=has..dots]".parse().unwrap();
    let pipe_path: FieldPath = "comps/li[@Class=has\\|pipe]".parse().unwrap();
    assert!(plan.unresolved.contains(&dots_path));
    assert!(plan.unresolved.contains(&pipe_path));
    assert!(
        plan.caveats
            .iter()
            .any(|c| matches!(c, Caveat::UnsafeXpathValue { path } if path == &dots_path))
    );
    assert!(
        plan.caveats
            .iter()
            .any(|c| matches!(c, Caveat::UnsafeXpathValue { path } if path == &pipe_path))
    );
}

#[test]
fn the_worked_example_plan_matches() {
    // `defaultLabelColor` is inherited on BIONICS's raw node (comes from
    // the `addedPartExampleSynth` template, itself resolved through
    // `inherit::resolve` — a real `TemplateSet`, not a hand-built
    // resolved tree).
    let templates = template_set(vec![(
        "HediffDef",
        "addedPartExampleSynth",
        "<HediffDef Name=\"addedPartExampleSynth\" Abstract=\"True\"><defaultLabelColor>(188,39,242)</defaultLabelColor></HediffDef>",
    )]);
    let core = owner(
        "ludeon.rimworld",
        r#"<HediffDef>
                 <label>bionic heart</label>
                 <labelNoun>a bionic heart</labelNoun>
                 <description>An installed bionic heart. It has synthetic muscle fibers.</description>
                 <defaultLabelColor>(0.6, 0.6, 1.0)</defaultLabelColor>
               </HediffDef>"#,
    );
    let bionics = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="addedPartExampleSynth">
                 <label>synthetic heart</label>
                 <labelNoun>a synthetic heart</labelNoun>
                 <description>An installed synthetic heart. It has synthetic muscle fibers.</description>
               </HediffDef>"#,
        &templates,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let chosen = choices(vec![
        (
            "label",
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        ),
        (
            "labelNoun",
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        ),
        (
            "description",
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        ),
        (
            "defaultLabelColor",
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        ),
    ]);

    let plan = plan_def_override(&diff, &bionics, &chosen);

    assert_eq!(plan.ops.len(), 4);
    assert!(plan.unresolved.is_empty());
    let replace_paths: BTreeSet<String> = plan
        .ops
        .iter()
        .filter_map(|op| match &op.op {
            PlanOp::Replace { path, .. } => Some(path.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        replace_paths,
        ["label", "labelNoun", "description"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert!(
        plan.ops.iter().any(
            |op| matches!(&op.op, PlanOp::Add { node, .. } if node.tag == "defaultLabelColor")
        )
    );
    for op in &plan.ops {
        assert_eq!(
            op.depends_on,
            [ModId::new("example.bionicsfork")].into_iter().collect()
        );
    }
}

// -- plan_patch_collision -------------------------------------------

fn plant_density_fixture(second_value: &str) -> (FieldTree, String) {
    let core = xml::parse(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/xml/core_temperate_forest.xml"
        ))
        .unwrap(),
    )
    .unwrap();
    let biomes_prehistoric_op = format!(
        r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <enabled>True</enabled>
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/BiomeDef[defName="TemperateForest"]/plantDensity</xpath>
                     <value><plantDensity>{second_value}</plantDensity></value>
                   </li>
                 </operations>
               </Operation>"#
    );
    (core, biomes_prehistoric_op)
}

#[test]
fn the_plant_density_collision_is_agreeing_when_every_candidate_matches() {
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density.xml"
    ))
    .unwrap();
    let (core, prehistoric_op) = plant_density_fixture("0.9");

    let flora = ModId::new("example.flora.core");
    let prehistoric = ModId::new("examplebiomes.biomes");
    let active: BTreeSet<ModId> = [flora.clone(), prehistoric.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let contributions = vec![
        PatchContribution {
            mod_id: &flora,
            operation_xml: &flora_op,
        },
        PatchContribution {
            mod_id: &prehistoric,
            operation_xml: &prehistoric_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[flora.clone(), prehistoric.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert!(plan.ops.is_empty());
    assert!(plan.unresolved.is_empty());
    assert!(
        plan.caveats
            .iter()
            .any(|c| matches!(c, Caveat::ModSettingDefault { .. }))
    );
}

#[test]
fn the_plant_density_collision_is_a_conflict_when_a_candidate_disagrees() {
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density.xml"
    ))
    .unwrap();
    let (core, prehistoric_op) = plant_density_fixture("1.2");

    let flora = ModId::new("example.flora.core");
    let prehistoric = ModId::new("examplebiomes.biomes");
    let active: BTreeSet<ModId> = [flora.clone(), prehistoric.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let contributions = vec![
        PatchContribution {
            mod_id: &flora,
            operation_xml: &flora_op,
        },
        PatchContribution {
            mod_id: &prehistoric,
            operation_xml: &prehistoric_op,
        },
    ];

    let unresolved_plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[flora.clone(), prehistoric.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;
    assert_eq!(
        unresolved_plan.unresolved,
        vec!["plantDensity".parse().unwrap()]
    );
    assert!(unresolved_plan.ops.is_empty());

    // `final_under_order` already equals `candidate(prehistoric)` here
    // (prehistoric already loads last), so choosing it would be a
    // no-op; choose `flora` instead to exercise a value that
    // actually differs from `final_under_order` (1.2).
    let choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::from([(
        "plantDensity".parse().unwrap(),
        MergeChoice::From {
            mod_id: flora.clone(),
        },
    )]);
    let decided_plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[flora.clone(), prehistoric.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &choices,
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;
    assert_eq!(decided_plan.ops.len(), 1);
    match &decided_plan.ops[0].op {
        PlanOp::Replace { path, node } => {
            assert_eq!(path.to_string(), "plantDensity");
            assert_eq!(node.content, Content::Text("0.9".to_string()));
        }
        other => panic!("expected Replace, got {other:?}"),
    }
    assert!(decided_plan.ops[0].depends_on.contains(&flora));
}

#[test]
fn a_drop_choice_on_a_collision_is_unresolved_with_a_caveat() {
    let (core, prehistoric_op) = plant_density_fixture("1.2");
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density.xml"
    ))
    .unwrap();
    let flora = ModId::new("example.flora.core");
    let prehistoric = ModId::new("examplebiomes.biomes");
    let active: BTreeSet<ModId> = [flora.clone(), prehistoric.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let contributions = vec![
        PatchContribution {
            mod_id: &flora,
            operation_xml: &flora_op,
        },
        PatchContribution {
            mod_id: &prehistoric,
            operation_xml: &prehistoric_op,
        },
    ];
    let choices: BTreeMap<FieldPath, MergeChoice> =
        BTreeMap::from([("plantDensity".parse().unwrap(), MergeChoice::Drop)]);

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[flora.clone(), prehistoric.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &choices,
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert!(plan.ops.is_empty());
    assert_eq!(plan.unresolved, vec!["plantDensity".parse().unwrap()]);
    assert!(
        plan.caveats
            .iter()
            .any(|c| matches!(c, Caveat::UnsupportedDrop { .. }))
    );
}

#[test]
fn an_unsupported_contribution_makes_plan_patch_collision_return_an_error() {
    let (core, _) = plant_density_fixture("0.9");
    let bad = ModId::new("bad.mod");
    let active: BTreeSet<ModId> = [bad.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let bad_op = r#"<Operation Class="Example.PatchOperationResearchPrereq">
                 <xpath>Defs/BiomeDef[defName="TemperateForest"]/plantDensity</xpath>
               </Operation>"#;
    let contributions = vec![PatchContribution {
        mod_id: &bad,
        operation_xml: bad_op,
    }];

    let result = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "TemperateForest".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("plantDensity".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: std::slice::from_ref(&bad),
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    });

    assert!(matches!(result, Err(ReplayError::Unsupported { .. })));
}

// -- keyed-map patch collisions --

/// Loads the keyed-map worked example: the raw `AridShrubland` node
/// plus its three mods' own top-level `<Operation>`s, split out of
/// the bundled fixture file the same way a real `DefSourceReader`
/// would hand each mod's own patch file separately.
fn arid_shrubland_wild_animals_fixture() -> (FieldTree, Vec<String>) {
    let core = xml::parse(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/xml/core_arid_shrubland_wild_animals.xml"
        ))
        .unwrap(),
    )
    .unwrap();
    let patches_xml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/arid_shrubland_wild_animals_patches.xml"
    ))
    .unwrap();
    let doc = roxmltree::Document::parse(&patches_xml)
        .unwrap_or_else(|error| panic!("well-formed fixture XML: {error}"));
    let operations: Vec<String> = doc
        .root_element()
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|node| patches_xml[node.range()].to_string())
        .collect();
    (core, operations)
}

#[test]
fn three_disjoint_map_adds_auto_resolve_with_no_choices_and_no_ops() {
    let (core, operations) = arid_shrubland_wild_animals_fixture();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone(), mod_c.clone()]
        .into_iter()
        .collect();
    let names = BTreeMap::new();
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: &operations[0],
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: &operations[1],
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: &operations[2],
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[mod_a.clone(), mod_b.clone(), mod_c.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    // "The three adds already union": three mods each adding their own
    // disjoint animal is already order-independent, so this resolves fully
    // automatically with nothing to emit
    // (`MergeState::Complete { op_count: 0 }`, computed by the caller from
    // an empty `ops`).
    assert!(
        plan.ops.is_empty(),
        "the real replay's own union already matches: {:?}",
        plan.ops
    );
    assert!(
        plan.unresolved.is_empty(),
        "every key is disjoint, so every entry auto-resolves: {:?}",
        plan.unresolved
    );
    assert!(
        plan.caveats.is_empty(),
        "no scoping leak into TemperateForest's own plantDensity: {:?}",
        plan.caveats
    );
}

#[test]
fn an_explicit_value_choice_on_one_map_entry_replaces_only_that_key() {
    let (core, operations) = arid_shrubland_wild_animals_fixture();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone(), mod_c.clone()]
        .into_iter()
        .collect();
    let names = BTreeMap::new();
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: &operations[0],
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: &operations[1],
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: &operations[2],
        },
    ];
    let choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::from([(
        "wildAnimals/XBM_Theropod".parse().unwrap(),
        MergeChoice::Value {
            text: "0.9".to_string(),
        },
    )]);

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        mods: &[mod_a.clone(), mod_b.clone(), mod_c.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &choices,
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    match &plan.ops[0].op {
        PlanOp::Replace { path, node } => {
            assert_eq!(path.to_string(), "wildAnimals/XBM_Theropod");
            assert_eq!(node.content, Content::Text("0.9".to_string()));
        }
        other => panic!("expected Replace, got {other:?}"),
    }
    assert!(
        plan.ops[0].depends_on.is_empty(),
        "a free-text Value choice contributes no extra mod beyond the (Core) def owner: {:?}",
        plan.ops[0].depends_on
    );
    assert!(plan.unresolved.is_empty());
}

#[test]
fn a_later_wholesale_replace_that_drops_an_earlier_added_key_is_restored_with_a_caveat() {
    // The "clobbered map entry" case: mod A adds `Donkey` to
    // a container that didn't exist yet; mod B, loading after A,
    // wholesale-replaces that same container with content that
    // doesn't carry `Donkey` forward. The entry still auto-resolves
    // (`OneSided { by: A }`, isolated replay), but the real, combined
    // replay no longer has it — restored with an `Add`.
    let target_raw = xml::parse("<BiomeDef><defName>X</defName></BiomeDef>").unwrap();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let add_op = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/BiomeDef[defName="X"]</xpath>
                 <value><wildAnimals><Donkey>0.2</Donkey></wildAnimals></value>
               </Operation>"#;
    let replace_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals</xpath>
                 <value><wildAnimals><Fox>0.5</Fox></wildAnimals></value>
               </Operation>"#;
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: add_op,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: replace_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        mods: &[mod_a.clone(), mod_b.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    match &plan.ops[0].op {
        PlanOp::Add { parent, node } => {
            assert_eq!(parent.to_string(), "wildAnimals");
            assert_eq!(node.tag, "Donkey");
            assert_eq!(node.content, Content::Text("0.2".to_string()));
        }
        other => panic!("expected an Add restoring Donkey, got {other:?}"),
    }
    assert!(plan.caveats.iter().any(|c| matches!(c, Caveat::ClobberedMapEntry { by, path } if by == &mod_a && path.to_string() == "wildAnimals/Donkey")
            ),
            "{:?}",
            plan.caveats
        );
}

#[test]
fn a_replace_of_a_key_only_an_earlier_add_created_is_not_silently_reverted() {
    // Mod A `Add`s a fresh key; mods B and C each `Replace` it to
    // their own value, C loading last. B's and C's own isolated
    // replays (only their own op, alone) match nothing — the key
    // doesn't exist without A's `Add` — so trusting those candidates
    // would read `Value::Absent`, auto-resolve the entry
    // `OneSided { by: A }` with no caveat, and emit a plan that
    // silently reverts the real, in-game value (C's, since C loads
    // last) back to A's.
    let target_raw = xml::parse("<BiomeDef><defName>X</defName></BiomeDef>").unwrap();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone(), mod_c.clone()]
        .into_iter()
        .collect();
    let names = BTreeMap::new();
    let add_op = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/BiomeDef[defName="X"]</xpath>
                 <value><wildAnimals><Raptor>0.3</Raptor></wildAnimals></value>
               </Operation>"#;
    let replace_b_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Raptor</xpath>
                 <value><Raptor>0.3</Raptor></value>
               </Operation>"#;
    let replace_c_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Raptor</xpath>
                 <value><Raptor>0.5</Raptor></value>
               </Operation>"#;
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: add_op,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: replace_b_op,
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: replace_c_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        mods: &[mod_a.clone(), mod_b.clone(), mod_c.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    // The real, full-order replay's own value is C's 0.5 (A adds
    // 0.3, B replaces to 0.3, C replaces to 0.5, C loads last) — a
    // genuine three-way conflict (A and B happen to agree on 0.3, C
    // differs) that needs the user's own input, never a silent,
    // auto-resolved `Add`/`Replace` that reverts it.
    assert_eq!(
        plan.unresolved,
        vec!["wildAnimals/Raptor".parse().unwrap()],
        "a Replace depending on another mod's earlier Add must surface as a real conflict, never silently auto-resolve: {plan:?}"
    );
    assert!(plan.ops.is_empty(), "{:?}", plan.ops);
}

#[test]
fn a_free_text_value_choice_on_an_attributed_map_entry_keeps_its_attributes() {
    // `<Donkey MayRequire="...">0.2</Donkey>` compares as
    // `Value::Item` (attributes preserved) and a `MergeChoice::From`
    // choice keeps them verbatim. Typed free text parses to a bare
    // `Value::Leaf` with no attribute syntax of its own, so
    // `build_node_for` must not emit a `Replace` with `attrs: {}` —
    // that would silently drop `MayRequire` and make the key apply on
    // an install that doesn't have the gated content active.
    let target_raw = xml::parse(r#"<BiomeDef><defName>X</defName><wildAnimals><Donkey MayRequire="ludeon.rimworld.odyssey">0.2</Donkey></wildAnimals></BiomeDef>"#)
        .unwrap();
    let mod_a = ModId::new("mod.a");
    let active: BTreeSet<ModId> = [mod_a.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let replace_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Donkey</xpath>
                 <value><Donkey MayRequire="ludeon.rimworld.odyssey">0.4</Donkey></value>
               </Operation>"#;
    let contributions = vec![PatchContribution {
        mod_id: &mod_a,
        operation_xml: replace_op,
    }];
    let choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::from([(
        "wildAnimals/Donkey".parse().unwrap(),
        MergeChoice::Value {
            text: "0.6".to_string(),
        },
    )]);

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        mods: std::slice::from_ref(&mod_a),
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &choices,
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    match &plan.ops[0].op {
        PlanOp::Replace { path, node } => {
            assert_eq!(path.to_string(), "wildAnimals/Donkey");
            assert_eq!(node.content, Content::Text("0.6".to_string()));
            assert_eq!(
                node.attrs.get("MayRequire").map(String::as_str),
                Some("ludeon.rimworld.odyssey"),
                "a free-text choice must not drop the entry's own MayRequire: {node:?}"
            );
        }
        other => panic!("expected Replace, got {other:?}"),
    }
}

#[test]
fn a_disjoint_map_add_later_touched_by_an_out_of_scope_mod_still_credits_only_its_own_mod() {
    // Pins the isolated-vs-`move_mod_last` switch in one direction:
    // forcing `is_map`/`looks_like_map` either way must fail a test.
    // A disjoint add whose key a fourth, out-of-scope mod later
    // replaces (in the real full order) needs an op — proving the
    // *isolated* candidate is actually used: under a
    // `move_mod_last` strategy, every one of A/B/C's own
    // reordered replays would see D's replace land *before* their
    // own irrelevant op reruns, misclassifying the field
    // `Conflict { A, B, C }` (unresolved) instead of the correct
    // isolated `OneSided { A }` (a clean `Replace`,
    // `depends_on == {A}`).
    let (core, mut operations) = arid_shrubland_wild_animals_fixture();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let mod_d = ModId::new("mod.d");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone(), mod_c.clone(), mod_d.clone()]
        .into_iter()
        .collect();
    let names = BTreeMap::new();
    let replace_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/XBM_Theropod</xpath>
                 <value><XBM_Theropod>0.99</XBM_Theropod></value>
               </Operation>"#
        .to_string();
    operations.push(replace_op);
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: &operations[0],
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: &operations[1],
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: &operations[2],
        },
        PatchContribution {
            mod_id: &mod_d,
            operation_xml: &operations[3],
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &core,
        // Deliberately excludes mod.d: it never has its own key of
        // the map to disjoint-union, only a compat `Replace` of
        // another mod's key — out of scope for *this* collision's
        // own participant list, the same way `Caveat::OutOfScopeOwners`
        // scenarios work, but its op still plays during the real
        // full-order replay via `contributions`.
        mods: &[mod_a.clone(), mod_b.clone(), mod_c.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    match &plan.ops[0].op {
        PlanOp::Replace { path, node } => {
            assert_eq!(path.to_string(), "wildAnimals/XBM_Theropod");
            assert_eq!(node.content, Content::Text("0.6".to_string()));
        }
        other => panic!("expected a Replace back to mod.a's own isolated value, got {other:?}"),
    }
    assert_eq!(
        plan.ops[0].depends_on,
        [mod_a.clone()].into_iter().collect::<BTreeSet<_>>(),
        "the isolated candidate must credit only mod.a, not b/c: {:?}",
        plan.ops[0].depends_on
    );
    assert!(plan.unresolved.is_empty(), "{:?}", plan.unresolved);
}

#[test]
fn a_mod_removing_the_only_key_while_another_adds_a_new_one_still_auto_resolves() {
    // `looks_like_map` is decided from `final_outcome.tree` alone, but
    // a per-mod *isolated* replay can itself disagree about whether
    // the container is still a map (here: mod.a's own isolated replay
    // removes the container's only key, leaving an empty `Record`
    // behind, not a `KeyedMap`). That is exactly the disagreement
    // `collision_fields`'s own second check exists to catch — but if
    // the isolated trees were the *only* candidate set it had to fall
    // back with, it would compare the whole subtree over isolated
    // candidates and report an unresolved `Conflict` for a change
    // that actually unions cleanly.
    let target_raw = xml::parse(
        "<BiomeDef><defName>X</defName><wildAnimals><Cobra>0.3</Cobra></wildAnimals></BiomeDef>",
    )
    .unwrap();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let remove_op = r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Cobra</xpath>
               </Operation>"#;
    let add_op = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals</xpath>
                 <value><Fox>0.5</Fox></value>
               </Operation>"#;
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: remove_op,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: add_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        mods: &[mod_a.clone(), mod_b.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert!(
        plan.unresolved.is_empty(),
        "removing the base's only key while another mod adds a fresh one must still auto-resolve, not fall back to an unresolved whole-container blob: {:?}",
        plan.unresolved
    );
}

#[test]
fn an_ordinary_non_map_field_whose_credited_mod_nets_absent_stays_unresolved() {
    // The silent-`Absent` no-op arm is guarded to `MapEntry`, so it
    // must not fire on the ordinary, non-map single-field path — here,
    // mod.a's own `move_mod_last`-reordered
    // candidate for `label` (a plain leaf, not a keyed map entry)
    // nets `Value::Absent` (mod.b's own out-of-scope `Add`, replayed
    // *before* mod.a's `Remove` once mod.a is moved last, creates a
    // duplicate `<label>` mod.a's `Remove` then wipes entirely) even
    // though the real, full-order replay's own final value is
    // present (mod.b's restored `label`). Unguarded, that would
    // silently become `Complete { op_count: 0 }` with the user never
    // told mod.a's own removal was dropped; the correct behaviour is
    // `unresolved`, exactly like every other field this crate can't
    // safely auto-resolve.
    let target_raw =
        xml::parse("<HediffDef><defName>X</defName><label>original</label></HediffDef>").unwrap();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone()].into_iter().collect();
    let names = BTreeMap::new();
    let remove_op = r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/HediffDef[defName="X"]/label</xpath>
               </Operation>"#;
    let add_op = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/HediffDef[defName="X"]</xpath>
                 <value><label>restored</label></value>
               </Operation>"#;
    // Real full order: mod.a removes the base's own label, then
    // mod.b restores one — the true, in-game value is present.
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: remove_op,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: add_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("label".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        // mod.b is deliberately out of scope for this field's own
        // collision participant list (same technique as the disjoint-add
        // and agreeing-clobber tests above) — its own `Add` still plays in the real
        // full-order replay via `contributions`, but only mod.a's
        // `Remove` is credited/reordered as this collision's own
        // candidate.
        mods: std::slice::from_ref(&mod_a),
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(
        plan.unresolved,
        vec!["label".parse().unwrap()],
        "must surface as unresolved, never a silent no-op: {plan:?}"
    );
    assert!(plan.ops.is_empty(), "{:?}", plan.ops);
}

#[test]
fn an_agreeing_clobbered_map_entry_is_restored_with_a_caveat_too() {
    // `Caveat::ClobberedMapEntry` fires for an `Agreeing` clobber too,
    // not only `DiffClass::OneSided` (two mods independently agreeing
    // on the same key, then a third mod's wholesale replace drops it):
    // the identical restoring `Add` with no caveat would leave the UI
    // with no way to explain the extra op.
    let target_raw = xml::parse(
        "<BiomeDef><defName>X</defName><wildAnimals><Donkey>0.1</Donkey></wildAnimals></BiomeDef>",
    )
    .unwrap();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let mod_c = ModId::new("mod.c");
    let active: BTreeSet<ModId> = [mod_a.clone(), mod_b.clone(), mod_c.clone()]
        .into_iter()
        .collect();
    let names = BTreeMap::new();
    let replace_a_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Donkey</xpath>
                 <value><Donkey>0.2</Donkey></value>
               </Operation>"#;
    let replace_b_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals/Donkey</xpath>
                 <value><Donkey>0.2</Donkey></value>
               </Operation>"#;
    let wholesale_replace_c_op = r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="X"]/wildAnimals</xpath>
                 <value><wildAnimals><Fox>0.5</Fox></wildAnimals></value>
               </Operation>"#;
    let contributions = vec![
        PatchContribution {
            mod_id: &mod_a,
            operation_xml: replace_a_op,
        },
        PatchContribution {
            mod_id: &mod_b,
            operation_xml: replace_b_op,
        },
        PatchContribution {
            mod_id: &mod_c,
            operation_xml: wholesale_replace_c_op,
        },
    ];

    let plan = plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "X".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".parse().unwrap()),
        def_owner: ModId::new("ludeon.rimworld"),
        target_raw: &target_raw,
        // mod.c is deliberately not a party to this field's own
        // collision — see the `mods` doc comment on the disjoint-add
        // test above for why a wholesale replacer outside this
        // exact sub-collision's own participant list is a realistic,
        // legitimate shape (its own op still plays in `contributions`).
        mods: &[mod_a.clone(), mod_b.clone()],
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;

    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    match &plan.ops[0].op {
        PlanOp::Add { parent, node } => {
            assert_eq!(parent.to_string(), "wildAnimals");
            assert_eq!(node.tag, "Donkey");
            assert_eq!(node.content, Content::Text("0.2".to_string()));
        }
        other => panic!("expected an Add restoring Donkey, got {other:?}"),
    }
    assert!(
        plan.caveats.iter().any(|c| matches!(c,
            Caveat::ClobberedMapEntry { by, path }
                if by == &mod_a && path.to_string() == "wildAnimals/Donkey"
        )),
        "an Agreeing clobber needs the same caveat a OneSided one gets: {:?}",
        plan.caveats
    );
}

#[test]
fn build_resolved_node_combines_fields_from_separate_owners_into_one_tree() {
    let core = owner(
        "ludeon.rimworld",
        "<HediffDef><label>bionic heart</label><hediffClass>Hediff_AddedPart</hediffClass></HediffDef>",
    );
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>synthetic heart</label><hediffClass>Hediff_AddedPart</hediffClass></HediffDef>",
    );
    let diff = crate::diff::three_way(&[core, bionics], &ModId::new("ludeon.rimworld")).unwrap();

    let node = build_resolved_node("HediffDef", &diff, &BTreeMap::new());

    assert_eq!(node.tag, "HediffDef");
    let Content::Children(children) = &node.content else {
        panic!("expected children");
    };
    let label = children.iter().find(|c| c.tag == "label").unwrap();
    assert_eq!(label.content, Content::Text("synthetic heart".to_string()));
    let hediff_class = children.iter().find(|c| c.tag == "hediffClass").unwrap();
    assert_eq!(
        hediff_class.content,
        Content::Text("Hediff_AddedPart".to_string())
    );
}

#[test]
fn build_resolved_node_merges_two_li_items_from_different_fields_under_one_container() {
    let core = owner("core", "<ThingDef></ThingDef>");
    let mod_a = owner(
        "mod.a",
        r#"<ThingDef><comps><li Class="A"/></comps></ThingDef>"#,
    );
    let diff = crate::diff::three_way(&[core, mod_a], &ModId::new("core")).unwrap();

    let node = build_resolved_node("ThingDef", &diff, &BTreeMap::new());

    let Content::Children(children) = &node.content else {
        panic!("expected children");
    };
    let comps = children.iter().find(|c| c.tag == "comps").unwrap();
    let Content::Children(items) = &comps.content else {
        panic!("expected li children");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].attrs.get("Class").map(String::as_str), Some("A"));
}

/// A patch collision whose op targets the def node itself gives
/// `build_resolved_node` a single field whose `path` is the empty
/// `FieldPath` (`plan_merge.rs`'s `path_key` default for `sub_path: None`)
/// and whose value is the whole resolved `Value::Item(node)`. Reaching
/// `build_chain` with `path.segments().len() == 0 ==
/// prefix.segments().len()` would panic on `0 - 1` (`attempt to subtract
/// with overflow`); instead the item is retagged to `def_type` and returned
/// directly, rather than folded under itself as a child.
#[test]
fn build_resolved_node_handles_a_whole_def_field_without_overflow() {
    let root_path = FieldPath::new(vec![]);
    let base_node = FieldNode {
        tag: "ThingDef".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(vec![FieldNode {
            tag: "label".to_string(),
            attrs: BTreeMap::new(),
            content: Content::Text("widget".to_string()),
        }]),
    };
    let candidate_node = FieldNode {
        tag: "ThingDef".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(vec![
            FieldNode {
                tag: "label".to_string(),
                attrs: BTreeMap::new(),
                content: Content::Text("widget".to_string()),
            },
            FieldNode {
                tag: "techLevel".to_string(),
                attrs: BTreeMap::new(),
                content: Content::Text("Industrial".to_string()),
            },
        ]),
    };
    let mod_a = ModId::new("a.mod");
    let diff = ThreeWayDiff {
        base: ModId::new("core"),
        fields: vec![FieldDiff {
            path: root_path,
            base: Value::Item(base_node),
            candidates: BTreeMap::from([(mod_a.clone(), Value::Item(candidate_node))]),
            class: DiffClass::OneSided { by: mod_a.clone() },
            is_list_item: false,
            entry: crate::diff::EntryKind::Leaf,
        }],
    };

    let node = build_resolved_node("ThingDef", &diff, &BTreeMap::new());

    assert_eq!(node.tag, "ThingDef");
    let Content::Children(children) = &node.content else {
        panic!("expected children");
    };
    assert!(
        children.iter().any(|c| c.tag == "techLevel"),
        "the whole-def field's own value must be used verbatim: {children:?}"
    );
}

/// [`build_chain`]'s own strict-prefix precondition: a `path` no
/// longer than `prefix` (the empty/empty case, and any
/// other caller bug that manages to violate it) must come back `None`
/// rather than underflow `path.segments().len() - 1`.
#[test]
fn build_chain_returns_none_when_path_is_no_longer_than_prefix() {
    let empty = FieldPath::new(vec![]);
    let inner = FieldNode {
        tag: "label".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text("widget".to_string()),
    };

    assert_eq!(build_chain(&empty, &empty, inner), None);
}

/// `plan_drop`'s container-vs-leaf split must not index an empty path —
/// a "length checked apart from the arithmetic it guards" shape is a
/// latent panic. No real caller reaches that split with an empty path,
/// for two independent reasons this test pins together:
/// `FieldTree::leaves()` never yields an empty-path field (so
/// `plan_def_override`/`resolved_field_values` never call `plan_drop`
/// this way), and even a direct call can't reach the container-split
/// branch at all, since `FieldTree::get`'s own "an empty path returns
/// the root itself" contract makes `has_raw` true unconditionally,
/// short-circuiting to `DropOutcome::Op(Remove)` first. The
/// `split_first`-based split does not *rely* on either invariant to
/// avoid indexing an empty slice, so a future change to either one
/// (say, `has_raw` gaining a third condition) can't quietly open a
/// panic.
#[test]
fn plan_drop_does_not_panic_on_an_empty_path() {
    let empty = FieldPath::new(vec![]);
    let winner = owner("a", "<ThingDef><label>widget</label></ThingDef>");

    let outcome = plan_drop(&empty, &winner, BTreeSet::new());

    assert!(matches!(outcome,
        DropOutcome::Op(PlannedOp {
            op: PlanOp::Remove { path },
            ..
        }) if path == empty
    ));
}

#[test]
fn build_resolved_node_leaves_an_unresolved_conflict_absent() {
    let core = owner(
        "core",
        "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
    );
    let a = owner("a", "<BiomeDef><plantDensity>0.9</plantDensity></BiomeDef>");
    let b = owner("b", "<BiomeDef><plantDensity>1.2</plantDensity></BiomeDef>");
    let diff = crate::diff::three_way(&[core, a, b], &ModId::new("core")).unwrap();

    let node = build_resolved_node("BiomeDef", &diff, &BTreeMap::new());

    let Content::Children(children) = &node.content else {
        panic!("expected children");
    };
    assert!(
        children.iter().all(|c| c.tag != "plantDensity"),
        "an unresolved conflict must not appear in the resolved node"
    );
}

/// The "final" value for a `DefOverride`, through `resolved_field_values`'
/// own public API. A cleanly `OneSided` field (no stored choice) must report the
/// winner's own resolved value.
#[test]
fn resolved_field_values_includes_a_cleanly_resolved_field() {
    let core = owner(
        "ludeon.rimworld",
        "<HediffDef><label>bionic heart</label></HediffDef>",
    );
    let bionics = owner(
        "example.bionicsfork",
        "<HediffDef><label>synthetic heart</label></HediffDef>",
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();

    let values = resolved_field_values(&diff, &bionics, &BTreeMap::new());

    assert_eq!(
        values.get(&"label".parse().unwrap()),
        Some(&Value::Leaf("synthetic heart".to_string()))
    );
}

/// A genuine, unresolved `Conflict` with no stored choice has no final
/// value at all — the caller's own "clearly-empty marker" case, never
/// a guess.
#[test]
fn resolved_field_values_excludes_an_unresolved_conflict_with_no_stored_choice() {
    let core = owner("core", "<ThingDef><label>wall</label></ThingDef>");
    let a = owner("a", "<ThingDef><label>stone wall</label></ThingDef>");
    let b = owner("b", "<ThingDef><label>brick wall</label></ThingDef>");
    let diff = crate::diff::three_way(&[core, a, b.clone()], &ModId::new("core")).unwrap();

    let values = resolved_field_values(&diff, &b, &BTreeMap::new());

    assert_eq!(
        values.get(&"label".parse().unwrap()),
        None,
        "an unresolved conflict must report no final value, not a guess"
    );
}

/// A field whose *chosen* value resolves fine but that
/// `plan_def_override` would itself abandon into `unresolved` must
/// also report no final value — otherwise `merge plan` prints a
/// confident `final` on one line and `UNRESOLVED` for the identical
/// path two lines below, the exact contradiction this column exists
/// to remove. This pins the `DropOutcome::Blocked` path specifically:
/// an explicit `MergeChoice::Drop` on a one-segment leaf that only
/// exists via inheritance (never in the winner's own raw XML) can't
/// be represented as a drop at all (`Caveat::UnsettableLeaf` — no
/// container to reconstruct minus a top-level leaf).
#[test]
fn resolved_field_values_excludes_a_drop_plan_def_override_would_itself_block() {
    let templates = template_set(vec![(
        "HediffDef",
        "Base",
        "<HediffDef><label>base label</label></HediffDef>",
    )]);
    let winner = owner_via_templates(
        "example.bionicsfork",
        r#"<HediffDef ParentName="Base"><defName>X</defName></HediffDef>"#,
        &templates,
    );
    assert_eq!(
        winner.raw.get(&"label".parse().unwrap()),
        None,
        "sanity check: label is purely inherited, never in the raw XML"
    );
    let diff = ThreeWayDiff {
        base: winner.mod_id.clone(),
        fields: vec![FieldDiff {
            path: "label".parse().unwrap(),
            base: Value::Leaf("base label".to_string()),
            candidates: BTreeMap::new(),
            class: DiffClass::Unchanged,
            is_list_item: false,
            entry: EntryKind::Leaf,
        }],
    };
    let drop_choice = choices(vec![("label", MergeChoice::Drop)]);

    let values = resolved_field_values(&diff, &winner, &drop_choice);

    assert_eq!(
        values.get(&"label".parse().unwrap()),
        None,
        "plan_def_override itself would block this exact drop \
             (Caveat::UnsettableLeaf) — reporting a final value here would \
             contradict its own UNRESOLVED row"
    );
}

// -- patch collisions where one mod's own operation fails -------------

const EXAMPLE_WALL: &str = "<ThingDef><defName>ExampleWall</defName><label>wall</label>\
     <designationCategory>Structure</designationCategory>\
     <researchPrerequisites><li>ExampleTechA</li></researchPrerequisites></ThingDef>";

fn op_on(operation_class: &str, sub_path: &str, value: &str) -> String {
    let value_element = if value.is_empty() {
        String::new()
    } else {
        format!("<value>{value}</value>")
    };
    format!(
        r#"<Operation Class="{operation_class}"><xpath>Defs/ThingDef[defName="ExampleWall"]/{sub_path}</xpath>{value_element}</Operation>"#
    )
}

/// Plans a collision on `sub_path` of `EXAMPLE_WALL`, with `ops` given as
/// `(mod id, operation xml)` in load order (a mod may appear more than
/// once). Replays through the real
/// patch evaluator and classifier — nothing about candidates is stubbed.
fn collide_on_example_wall(
    ops: &[(&str, String)],
    sub_path: &str,
) -> crate::plan::PatchCollisionOutcome {
    let target_raw = xml::parse(EXAMPLE_WALL).unwrap();
    let op_owners: Vec<ModId> = ops.iter().map(|(id, _)| ModId::new(*id)).collect();
    let mut mod_ids: Vec<ModId> = Vec::new();
    for owner in &op_owners {
        if !mod_ids.contains(owner) {
            mod_ids.push(owner.clone());
        }
    }
    let active: BTreeSet<ModId> = mod_ids.iter().cloned().collect();
    let names = BTreeMap::new();
    let contributions: Vec<PatchContribution<'_>> = op_owners
        .iter()
        .zip(ops)
        .map(|(mod_id, (_, operation_xml))| PatchContribution {
            mod_id,
            operation_xml,
        })
        .collect();

    plan_patch_collision(PatchCollisionInput {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "ExampleWall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some(sub_path.parse().unwrap()),
        def_owner: ModId::new("example.base"),
        target_raw: &target_raw,
        mods: &mod_ids,
        contributions: &contributions,
        active_mods: &active,
        mod_names_by_display: &names,
        def_exists: crate::patch_eval::def_existence_unknown(),
        choices: &BTreeMap::new(),
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
}

fn only_field(outcome: &crate::plan::PatchCollisionOutcome) -> &FieldDiff {
    match outcome.fields.as_slice() {
        [only] => only,
        other => panic!("expected exactly one field, got {other:?}"),
    }
}

fn leaf_text(text: &str) -> Value {
    Value::Leaf(text.to_string())
}

#[test]
fn a_replace_whose_target_another_mod_removed_is_a_conflict_not_an_agreement() {
    let outcome = collide_on_example_wall(
        &[
            (
                "example.remover",
                op_on("PatchOperationRemove", "designationCategory", ""),
            ),
            (
                "example.replacer",
                op_on(
                    "PatchOperationReplace",
                    "designationCategory",
                    "<designationCategory>Production</designationCategory>",
                ),
            ),
        ],
        "designationCategory",
    );

    let field = only_field(&outcome);
    assert!(
        matches!(field.class, DiffClass::Conflict { .. }),
        "remover vs replacer must ask, got {:?}",
        field.class
    );
    assert_eq!(
        field.candidates.get(&ModId::new("example.replacer")),
        Some(&leaf_text("Production")),
        "the replacer's candidate is its own op replayed alone"
    );
    assert_eq!(
        field.candidates.get(&ModId::new("example.remover")),
        Some(&Value::Absent)
    );
    assert_eq!(
        outcome.plan.unresolved,
        vec!["designationCategory".parse().unwrap()]
    );
}

#[test]
fn a_replace_of_a_removed_list_keeps_the_replacers_own_items_as_its_candidate() {
    let outcome = collide_on_example_wall(
        &[
            (
                "example.remover",
                op_on("PatchOperationRemove", "researchPrerequisites", ""),
            ),
            (
                "example.replacer",
                op_on(
                    "PatchOperationReplace",
                    "researchPrerequisites",
                    "<researchPrerequisites><li>ExampleTechB</li></researchPrerequisites>",
                ),
            ),
        ],
        "researchPrerequisites",
    );

    let field = only_field(&outcome);
    assert!(
        matches!(field.class, DiffClass::Conflict { .. }),
        "got {:?}",
        field.class
    );
    let candidate = field
        .candidates
        .get(&ModId::new("example.replacer"))
        .unwrap();
    assert_ne!(candidate, &Value::Absent);
    assert!(!outcome.plan.unresolved.is_empty());
}

#[test]
fn the_removal_and_replacement_conflict_in_either_load_order() {
    let remove = op_on("PatchOperationRemove", "designationCategory", "");
    let replace = op_on(
        "PatchOperationReplace",
        "designationCategory",
        "<designationCategory>Production</designationCategory>",
    );

    let replacer_first = collide_on_example_wall(
        &[
            ("example.replacer", replace.clone()),
            ("example.remover", remove.clone()),
        ],
        "designationCategory",
    );

    let field = only_field(&replacer_first);
    assert!(
        matches!(field.class, DiffClass::Conflict { .. }),
        "got {:?}",
        field.class
    );
}

#[test]
fn two_replacers_with_different_values_are_still_a_conflict_with_their_own_values() {
    let outcome = collide_on_example_wall(
        &[
            (
                "example.first",
                op_on("PatchOperationReplace", "label", "<label>first</label>"),
            ),
            (
                "example.second",
                op_on("PatchOperationReplace", "label", "<label>second</label>"),
            ),
        ],
        "label",
    );

    let field = only_field(&outcome);
    assert!(matches!(field.class, DiffClass::Conflict { .. }));
    assert_eq!(
        field.candidates.get(&ModId::new("example.first")),
        Some(&leaf_text("first"))
    );
    assert_eq!(
        field.candidates.get(&ModId::new("example.second")),
        Some(&leaf_text("second"))
    );
}

#[test]
fn a_replace_that_cannot_apply_even_alone_stays_agreeing_with_its_failed_op_caveat() {
    // The replacer's xpath names a field the base def never had, so its
    // op is a dead target in every order: nothing to recover by replaying
    // it alone.
    let outcome = collide_on_example_wall(
        &[
            (
                "example.remover",
                op_on("PatchOperationRemove", "designationCategory", ""),
            ),
            (
                "example.dead",
                op_on(
                    "PatchOperationReplace",
                    "designationCategory/missing",
                    "<missing>x</missing>",
                ),
            ),
        ],
        "designationCategory",
    );

    let field = only_field(&outcome);
    assert_eq!(
        field.candidates.get(&ModId::new("example.dead")),
        Some(&Value::Absent)
    );
    assert!(
        outcome.plan.caveats.iter().any(|caveat| matches!(
            caveat,
            Caveat::FailedOp { mod_id, .. } if mod_id == &ModId::new("example.dead")
        )),
        "{:?}",
        outcome.plan.caveats
    );
}

#[test]
fn a_failed_op_elsewhere_in_the_def_does_not_hide_a_replacers_own_candidate() {
    // The replacer also ships an operation on a stat the def never had,
    // which fails in every order. Only a failure at or under the contested
    // field says the replacer's op on it did not run.
    let outcome = collide_on_example_wall(
        &[
            (
                "example.remover",
                op_on("PatchOperationRemove", "designationCategory", ""),
            ),
            (
                "example.replacer",
                op_on(
                    "PatchOperationReplace",
                    "designationCategory",
                    "<designationCategory>Production</designationCategory>",
                ),
            ),
            (
                "example.replacer",
                op_on(
                    "PatchOperationReplace",
                    "statBases/MissingStat",
                    "<MissingStat>1</MissingStat>",
                ),
            ),
        ],
        "designationCategory",
    );

    let field = only_field(&outcome);
    assert_eq!(
        field.candidates.get(&ModId::new("example.replacer")),
        Some(&leaf_text("Production")),
        "the replacer's candidate is its own op replayed alone"
    );
}
