//! Tests for the merge mod emitter.

use super::patches::render_xpath;
use super::*;
use crate::diff::OwnerVersion;
use crate::patch_behaviours::PatchOperationBehaviours;
use crate::plan::{DefKey, PlanOp};
use crate::plan::{
    MergeChoice, PatchCollisionInput, PlannedOp, plan_def_override, plan_patch_collision,
};
use crate::tree::{Content, FieldNode, FieldPath};
use crate::xml;
use rim_analyzer::domain::Selector;
use std::collections::BTreeMap as Map;
use std::path::Path;

/// Every `<tag>...</tag>` value inside `text`, in document order — for
/// asserting an `About.xml` section's contents by *exact* list, not
/// `.contains`, since a substring check alone can't tell "present" from
/// "present and nothing else".
fn extract_tag_values<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let after_open = &rest[start + open.len()..];
        let Some(end) = after_open.find(&close) else {
            break;
        };
        values.push(&after_open[..end]);
        rest = &after_open[end + close.len()..];
    }
    values
}

/// An empty-path check: a whole-def field's [`PlanOp::Replace`] carries an empty
/// [`FieldPath`] (see `plan.rs`'s `build_resolved_node`/`plan_patch_collision`'s
/// `path_key` default for `sub_path: None`). Unlike `build_chain`,
/// `render_xpath` has no arithmetic on `path.segments().len()` at
/// all — it only appends one `/`-joined segment per iteration of an
/// empty loop — so an empty path renders the bare def predicate,
/// `Defs/<def_type>[defName="<def_name>"]`, which addresses the def
/// node itself and parses back through the replay's xpath grammar
/// exactly like any other target. This pins that an empty path is safe
/// here.
#[test]
fn render_xpath_of_an_empty_path_addresses_the_def_node_itself() {
    let key = DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Widget".to_string(),
    };

    let xpath = render_xpath(&key, Selector::DefName, &FieldPath::new(vec![]));

    assert_eq!(xpath, r#"Defs/ThingDef[defName="Widget"]"#);
}

fn owner_with_resolved(id: &str, raw_xml: &str, resolved_xml: &str) -> OwnerVersion {
    OwnerVersion {
        mod_id: ModId::new(id),
        raw: xml::parse(raw_xml).unwrap(),
        resolved: xml::parse(resolved_xml).unwrap(),
        inherited: None,
    }
}

fn worked_example_plan() -> MergePlan {
    let core = owner_with_resolved(
        "ludeon.rimworld",
        r#"<HediffDef>
                 <label>bionic heart</label>
                 <labelNoun>a bionic heart</labelNoun>
                 <description>An installed bionic heart. It has synthetic muscle fibers.</description>
               </HediffDef>"#,
        r#"<HediffDef>
                 <label>bionic heart</label>
                 <labelNoun>a bionic heart</labelNoun>
                 <description>An installed bionic heart. It has synthetic muscle fibers.</description>
                 <defaultLabelColor>(0.6, 0.6, 1.0)</defaultLabelColor>
               </HediffDef>"#,
    );
    let bionics = owner_with_resolved(
        "example.bionicsfork",
        r#"<HediffDef>
                 <defName>BionicHeart</defName>
                 <label>synthetic heart</label>
                 <labelNoun>a synthetic heart</labelNoun>
                 <description>An installed synthetic heart. It has synthetic muscle fibers.</description>
               </HediffDef>"#,
        r#"<HediffDef>
                 <defName>BionicHeart</defName>
                 <label>synthetic heart</label>
                 <labelNoun>a synthetic heart</labelNoun>
                 <description>An installed synthetic heart. It has synthetic muscle fibers.</description>
                 <defaultLabelColor>(188,39,242)</defaultLabelColor>
               </HediffDef>"#,
    );
    let diff =
        crate::diff::three_way(&[core, bionics.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let choices: Map<FieldPath, MergeChoice> = [
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
    ]
    .into_iter()
    .map(|(path, choice)| (path.parse().unwrap(), choice))
    .collect();
    plan_def_override(&diff, &bionics, &choices)
}

fn identity() -> GeneratedModIdentity {
    GeneratedModIdentity::for_profile("3f9a1c2b7d5e")
}

const PROFILE_DESCRIPTION: &str = "Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.";

fn base_input<'a>(
    identity: &'a GeneratedModIdentity,
    plans: &'a [MergePlan],
    mod_names: &'a Map<ModId, String>,
) -> EmitInput<'a> {
    EmitInput {
        about: AboutSpec {
            identity,
            author: "Rimmerge",
            description: PROFILE_DESCRIPTION,
            dependencies: Dependencies::FromContent,
        },
        game_version: "1.6",
        plans,
        defs: &[],
        assets: &[],
        mod_names,
        provenance: Provenance::ProfileMerge {
            generated_at: "2026-09-05T00:00:00Z",
            profile_hash: "3f9a1c2b7d5e",
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    }
}

fn patch_scope() -> BTreeSet<ModId> {
    [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
        .into_iter()
        .collect()
}

/// The patch counterpart of [`base_input`]: `Dependencies::Exactly`
/// over `scope` and a timestamp-free [`Provenance::CompatPatch`].
fn patch_input<'a>(
    identity: &'a GeneratedModIdentity,
    plans: &'a [MergePlan],
    mod_names: &'a Map<ModId, String>,
    scope: &'a BTreeSet<ModId>,
) -> EmitInput<'a> {
    EmitInput {
        about: AboutSpec {
            identity,
            author: "sample",
            description: "Compatibility patch generated by Rimmerge from field-level merge decisions.",
            dependencies: Dependencies::Exactly(scope),
        },
        game_version: "1.6",
        plans,
        defs: &[],
        assets: &[],
        mod_names,
        provenance: Provenance::CompatPatch {
            patch_id: "3f9a1c02be77",
            profile_hash: "3f9a1c2b7d5e",
            scope,
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    }
}

#[test]
fn about_xml_matches_the_worked_example_shape() {
    let identity = identity();
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    let input = base_input(&identity, &plans, &mod_names);

    let rendered = render(&input).unwrap();
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .unwrap();
    let FileContent::Text(text) = &about.content else {
        panic!("About.xml must be text")
    };

    insta::assert_snapshot!("about_xml_worked_example", text);
    assert!(text.contains("rimmerge.merge.3f9a1c2b7d5e"));
    assert!(text.contains("example.bionicsfork"));
    assert!(!text.contains("ludeon.rimworld"));
}

#[test]
fn patches_file_matches_the_worked_example_shape() {
    let identity = identity();
    let plans = vec![worked_example_plan()];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);

    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_HediffDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };

    insta::assert_snapshot!("patches_hediffdef_worked_example", text);
    assert!(text.contains(r#"Defs/HediffDef[defName="BionicHeart"]/label"#));
    assert!(text.contains("PatchOperationReplace"));
    assert!(text.contains("PatchOperationAdd"));
    assert!(text.contains(r#"MayRequire="example.bionicsfork""#));
}

#[test]
fn the_biomedef_plant_density_conflict_case_matches_the_worked_example_shape() {
    // The "had Prehistoric written 1.2" variant: choosing
    // `From(examplebiomes.biomes)` on the plantDensity
    // conflict.
    let core = xml::parse(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/xml/core_temperate_forest.xml"
        ))
        .unwrap(),
    )
    .unwrap();
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density.xml"
    ))
    .unwrap();
    let prehistoric_op = r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <enabled>True</enabled>
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/BiomeDef[defName="TemperateForest"]/plantDensity</xpath>
                     <value><plantDensity>1.2</plantDensity></value>
                   </li>
                 </operations>
               </Operation>"#;
    let flora = ModId::new("example.flora.core");
    let prehistoric = ModId::new("examplebiomes.biomes");
    let active: BTreeSet<ModId> = [flora.clone(), prehistoric.clone()].into_iter().collect();
    let names = Map::new();
    // The selected order has Flora loading *last* — so
    // `final_under_order` is Flora's 0.9, and choosing
    // Prehistoric's 1.2 is a genuine change from it.
    let contributions = vec![
        crate::patch_eval::PatchContribution {
            mod_id: &prehistoric,
            operation_xml: prehistoric_op,
        },
        crate::patch_eval::PatchContribution {
            mod_id: &flora,
            operation_xml: &flora_op,
        },
    ];
    let choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::from([(
        "plantDensity".parse().unwrap(),
        MergeChoice::From {
            mod_id: prehistoric.clone(),
        },
    )]);

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

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_BiomeDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };

    insta::assert_snapshot!("patches_biomedef_plant_density_conflict", text);
    assert!(text.contains(r#"MayRequire="examplebiomes.biomes""#));
    assert!(text.contains("<plantDensity>1.2</plantDensity>"));
}

#[test]
fn xml_special_characters_in_a_value_are_escaped_exactly_once() {
    let core = owner_with_resolved(
        "ludeon.rimworld",
        "<ThingDef><label>a</label></ThingDef>",
        "<ThingDef><label>a</label></ThingDef>",
    );
    let other = owner_with_resolved(
        "mod.a",
        "<ThingDef><label>Tom &amp; Jerry &lt;3&gt;</label></ThingDef>",
        "<ThingDef><label>Tom &amp; Jerry &lt;3&gt;</label></ThingDef>",
    );
    let diff =
        crate::diff::three_way(&[core, other.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    // A free-text choice distinct from what either owner's own
    // resolved value already is, so it actually needs an op.
    let choices: Map<FieldPath, MergeChoice> = [(
        "label".parse().unwrap(),
        MergeChoice::Value {
            text: "Tom & Jerry <3> vs. Ann & Bob".to_string(),
        },
    )]
    .into_iter()
    .collect();
    let plan = plan_def_override(&diff, &other, &choices);
    assert!(!plan.ops.is_empty());

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_ThingDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };
    // Escaped exactly once: neither raw nor double-escaped.
    assert!(text.contains("Tom &amp; Jerry &lt;3&gt; vs. Ann &amp; Bob"));
    assert!(!text.contains("Tom & Jerry <3> vs. Ann & Bob"));
    assert!(!text.contains("&amp;amp;"));
    assert!(!text.contains("&amp;lt;"));
}

#[test]
fn an_xpath_value_containing_a_double_quote_is_single_quoted() {
    let core = owner_with_resolved(
        "ludeon.rimworld",
        r#"<ThingDef><comps><li Class="A"><v>1</v></li></comps></ThingDef>"#,
        r#"<ThingDef><comps><li Class="A"><v>1</v></li></comps></ThingDef>"#,
    );
    let other = owner_with_resolved(
        "mod.a",
        r#"<ThingDef><comps><li Class="Has&quot;Quote"><v>2</v></li></comps></ThingDef>"#,
        r#"<ThingDef><comps><li Class="Has&quot;Quote"><v>2</v></li></comps></ThingDef>"#,
    );
    let diff =
        crate::diff::three_way(&[core, other.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let choices: Map<FieldPath, MergeChoice> = [(
        "comps/li[@Class=Has\\\"Quote]".parse().unwrap(),
        MergeChoice::Value {
            text: "<li Class=\"Has&quot;Quote\"><v>3</v></li>".to_string(),
        },
    )]
    .into_iter()
    .collect();
    let plan = plan_def_override(&diff, &other, &choices);
    assert!(!plan.ops.is_empty());

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_ThingDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };
    assert!(text.contains(r#"li[@Class='Has"Quote']"#));
}

#[test]
fn rendering_twice_is_byte_identical_except_rimmerge_json_is_excluded_from_the_comparison() {
    let identity = identity();
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    let input = base_input(&identity, &plans, &mod_names);

    let first = render(&input).unwrap();
    let second = render(&input).unwrap();

    let deterministic = |rendered: &RenderedMod| -> Vec<RenderedFile> {
        rendered
            .files
            .iter()
            .filter(|f| f.relative_path != Path::new("rimmerge.json"))
            .cloned()
            .collect()
    };
    assert_eq!(deterministic(&first), deterministic(&second));
}

#[test]
fn rimmerge_json_uses_json_escaping_not_xml_escaping() {
    let identity = identity();
    let plans: Vec<MergePlan> = vec![];
    let mod_names = Map::new();
    let mut input = base_input(&identity, &plans, &mod_names);
    input.provenance = Provenance::ProfileMerge {
        generated_at: "2026-09-05T00:00:00Z",
        profile_hash: "3f9a1c2b7d5e",
        decisions_sha256: "has\"quote\\and\nnewline",
    };

    let rendered = render(&input).unwrap();
    let json_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("rimmerge.json"))
        .unwrap();
    let FileContent::Text(text) = &json_file.content else {
        panic!("rimmerge.json must be text")
    };
    assert!(text.contains(r#"has\"quote\\and\nnewline"#));
    assert!(text.contains(r#""kind":"merge""#));
    assert!(!text.contains("&quot;"));
    // Exactly one raw newline in the whole file (the file's own
    // trailing one) — the value's own newline must be the escaped
    // `\n` two-character sequence, not an embedded raw line break.
    assert_eq!(text.matches('\n').count(), 1);
}

#[test]
fn an_incomplete_plan_is_rejected() {
    let mut plan = worked_example_plan();
    plan.unresolved.push("label".parse().unwrap());
    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);

    let result = render(&input);
    assert!(matches!(result, Err(EmitError::IncompletePlan { .. })));
}

#[test]
fn a_plan_with_no_ops_emits_no_patch_file() {
    let core = owner_with_resolved(
        "ludeon.rimworld",
        "<ThingDef><label>a</label></ThingDef>",
        "<ThingDef><label>a</label></ThingDef>",
    );
    let same = owner_with_resolved(
        "mod.a",
        "<ThingDef><label>a</label></ThingDef>",
        "<ThingDef><label>a</label></ThingDef>",
    );
    let diff =
        crate::diff::three_way(&[core, same.clone()], &ModId::new("ludeon.rimworld")).unwrap();
    let plan = plan_def_override(&diff, &same, &Map::new());
    assert!(plan.ops.is_empty());

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    assert!(
        !rendered
            .files
            .iter()
            .any(|f| f.relative_path.to_string_lossy().starts_with("Patches/"))
    );
}

#[test]
fn asset_copies_land_under_textures() {
    let identity = identity();
    let plans: Vec<MergePlan> = vec![];
    let mod_names = Map::new();
    let mut input = base_input(&identity, &plans, &mod_names);
    let assets = vec![AssetCopy {
        texture_path: "ui/foo".to_string(),
        from: ModId::new("mod.a"),
        source: PathBuf::from("C:/mods/a/Textures/UI/Foo.png"),
        relative_target: PathBuf::from("UI/Foo.png"),
    }];
    input.assets = &assets;

    let rendered = render(&input).unwrap();
    let asset_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Textures/UI/Foo.png"))
        .unwrap();
    assert_eq!(
        asset_file.content,
        FileContent::CopyFrom(PathBuf::from("C:/mods/a/Textures/UI/Foo.png"))
    );
}

/// The game never reads a top-level `PatchOperationSequence`'s own
/// `MayRequire` (`DirectXmlToObject.ObjectFromXml`, not `ListFromXml`);
/// only a `<li>` inside its `<operations>` list is read that way. So the
/// wrapper must never carry the attribute, and every `<li>` must carry
/// its *own* op's full `depends_on` — never a "shared, so omit it"
/// optimization, since there is no wrapper-level gate left to cover the
/// shared part.
#[test]
fn s5_every_li_carries_its_own_full_may_require_never_the_wrapper() {
    // Two ops sharing mod `a` but each also depending on a different
    // second mod.
    let plan = MergePlan {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        winner: ModId::new("a"),
        owners: vec![ModId::new("a"), ModId::new("b"), ModId::new("c")],
        ops: vec![
            PlannedOp {
                op: PlanOp::Replace {
                    path: "x".parse().unwrap(),
                    node: FieldNode {
                        tag: "x".to_string(),
                        attrs: BTreeMap::new(),
                        content: Content::Text("1".to_string()),
                    },
                },
                depends_on: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
            },
            PlannedOp {
                op: PlanOp::Replace {
                    path: "y".parse().unwrap(),
                    node: FieldNode {
                        tag: "y".to_string(),
                        attrs: BTreeMap::new(),
                        content: Content::Text("2".to_string()),
                    },
                },
                depends_on: [ModId::new("a"), ModId::new("c")].into_iter().collect(),
            },
        ],
        unresolved: vec![],
        caveats: vec![],
    };

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_ThingDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };

    let sequence_line = text
        .lines()
        .find(|line| line.contains("PatchOperationSequence"))
        .unwrap();
    // The wrapper carries no gate at all — the game would never read it
    // there — while each `<li>` carries its own full dependency set.
    assert!(!sequence_line.contains("MayRequire"));
    assert!(text.contains(r#"MayRequire="a,b""#));
    assert!(text.contains(r#"MayRequire="a,c""#));
}

/// The BionicHeart worked-example shape: every op shares the exact same
/// single dependency. Before this fix, an op whose `depends_on` equalled
/// the sequence-wide intersection got no `<li>`-level gate at all,
/// relying entirely on the (unread) wrapper gate — silently ungated in
/// the real game. Now every `<li>` gets its own gate regardless of
/// whether it happens to match every sibling's.
#[test]
fn every_op_sharing_the_identical_single_dependency_still_gets_its_own_li_gate() {
    let plan = MergePlan {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        winner: ModId::new("a"),
        owners: vec![ModId::new("a"), ModId::new("ludeon.rimworld")],
        ops: vec![
            PlannedOp {
                op: PlanOp::Replace {
                    path: "x".parse().unwrap(),
                    node: FieldNode {
                        tag: "x".to_string(),
                        attrs: BTreeMap::new(),
                        content: Content::Text("1".to_string()),
                    },
                },
                depends_on: [ModId::new("a")].into_iter().collect(),
            },
            PlannedOp {
                op: PlanOp::Replace {
                    path: "y".parse().unwrap(),
                    node: FieldNode {
                        tag: "y".to_string(),
                        attrs: BTreeMap::new(),
                        content: Content::Text("2".to_string()),
                    },
                },
                depends_on: [ModId::new("a")].into_iter().collect(),
            },
        ],
        unresolved: vec![],
        caveats: vec![],
    };

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_ThingDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };

    let sequence_line = text
        .lines()
        .find(|line| line.contains("PatchOperationSequence"))
        .unwrap();
    let li_lines: Vec<&str> = text.lines().filter(|line| line.contains("<li ")).collect();

    assert!(!sequence_line.contains("MayRequire"));
    assert_eq!(
        li_lines.len(),
        2,
        "both ops render their own <li>: {li_lines:?}"
    );
    for li_line in li_lines {
        assert!(
            li_line.contains(r#"MayRequire="a""#),
            "every <li> must carry its own gate, identical siblings included: {li_line}"
        );
    }
}

#[test]
fn owners_appear_in_the_patches_file_header_comment() {
    let plan = worked_example_plan();
    assert_eq!(
        plan.owners,
        vec![
            ModId::new("example.bionicsfork"),
            ModId::new("ludeon.rimworld")
        ]
    );

    let identity = identity();
    let plans = vec![plan];
    let mod_names = Map::new();
    let input = base_input(&identity, &plans, &mod_names);
    let rendered = render(&input).unwrap();
    let patch_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Patches/rimmerge_HediffDef.xml"))
        .unwrap();
    let FileContent::Text(text) = &patch_file.content else {
        panic!("patch file must be text")
    };
    assert!(text.contains("owners: example.bionicsfork,ludeon.rimworld"));
}

fn patch_identity() -> GeneratedModIdentity {
    GeneratedModIdentity {
        package_id: ModId::new("sample.abcompat"),
        folder_name: "sample_abcompat".to_string(),
        display_name: "A + B Compatibility".to_string(),
    }
}

#[test]
fn about_xml_with_exactly_dependencies_declares_precisely_the_scope() {
    // `worked_example_plan`'s one op depends on `example.bionicsfork` alone;
    // the scope also carries a second mod that contributes nothing —
    // `Exactly` must still declare it (unlike `FromContent`, which
    // would only ever list mods the content actually touches).
    let identity = patch_identity();
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    mod_names.insert(ModId::new("other.mod"), "Other Mod".to_string());
    let scope: BTreeSet<ModId> = [ModId::new("example.bionicsfork"), ModId::new("other.mod")]
        .into_iter()
        .collect();
    let input = patch_input(&identity, &plans, &mod_names, &scope);

    let rendered = render(&input).unwrap();
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .unwrap();
    let FileContent::Text(text) = &about.content else {
        panic!("About.xml must be text")
    };

    insta::assert_snapshot!("about_xml_patch_exactly_scope", text);
    assert!(text.contains("sample.abcompat"));
    assert!(text.contains("example.bionicsfork"));
    // `other.mod` contributes no op at all, but `Exactly` declares it
    // anyway — the whole point of the mode.
    assert!(text.contains("other.mod"));
    assert!(!text.contains("ludeon.rimworld"));
}

#[test]
fn an_op_depending_on_a_mod_outside_the_declared_scope_is_refused() {
    // `worked_example_plan`'s op depends on `example.bionicsfork`, which this
    // scope deliberately omits — a hand-edited decision file naming an
    // out-of-scope owner, the shape `EmitError::UndeclaredDependency`
    // exists to catch. A second plan depending on a *different*
    // out-of-scope mod, `aaa.mod` — alphabetically before
    // `example.bionicsfork` but listed *second* in `plans` — proves the
    // reported offender is the alphabetically first id in `used`
    // (`render`'s own `BTreeSet` iteration order), not just whichever
    // plan happens to be walked first.
    let identity = patch_identity();
    let second_core = owner_with_resolved(
        "ludeon.rimworld",
        "<ThingDef><label>a</label></ThingDef>",
        "<ThingDef><label>a</label></ThingDef>",
    );
    let second_other = owner_with_resolved(
        "aaa.mod",
        "<ThingDef><label>b</label></ThingDef>",
        "<ThingDef><label>b</label></ThingDef>",
    );
    let second_diff = crate::diff::three_way(
        &[second_core, second_other.clone()],
        &ModId::new("ludeon.rimworld"),
    )
    .unwrap();
    let second_choices: Map<FieldPath, MergeChoice> = [(
        "label".parse().unwrap(),
        MergeChoice::Value {
            text: "c".to_string(),
        },
    )]
    .into_iter()
    .collect();
    let second_plan = plan_def_override(&second_diff, &second_other, &second_choices);
    assert!(!second_plan.ops.is_empty());

    let plans = vec![worked_example_plan(), second_plan];
    let mod_names = Map::new();
    let scope: BTreeSet<ModId> = [ModId::new("other.mod")].into_iter().collect();
    let input = patch_input(&identity, &plans, &mod_names, &scope);

    let result = render(&input);
    assert_eq!(
        result,
        Err(EmitError::UndeclaredDependency {
            mod_id: ModId::new("aaa.mod")
        })
    );
}

#[test]
fn rimmerge_json_for_a_compat_patch_matches_the_kind_patch_shape() {
    let identity = patch_identity();
    let plans: Vec<MergePlan> = vec![];
    let mod_names = Map::new();
    let scope = patch_scope();
    let input = patch_input(&identity, &plans, &mod_names, &scope);

    let rendered = render(&input).unwrap();
    let json_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("rimmerge.json"))
        .unwrap();
    let FileContent::Text(text) = &json_file.content else {
        panic!("rimmerge.json must be text")
    };

    insta::assert_snapshot!("rimmerge_json_compat_patch", text);
    assert!(text.contains(r#""kind":"patch""#));
    assert!(text.contains(r#""patchId":"3f9a1c02be77""#));
    assert!(text.contains(r#""scope":["fixture.moda","fixture.modb"]"#));
    assert!(!text.contains("generatedAt"));
}

#[test]
fn a_compat_patch_render_carries_no_timestamp_and_two_renders_are_fully_identical() {
    let identity = patch_identity();
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    let scope: BTreeSet<ModId> = [ModId::new("example.bionicsfork")].into_iter().collect();
    let input = patch_input(&identity, &plans, &mod_names, &scope);

    let first = render(&input).unwrap();
    let second = render(&input).unwrap();
    // Unlike a `ProfileMerge` render (`rendering_twice_is_byte_identical_except_rimmerge_json_is_excluded_from_the_comparison`
    // above), nothing needs excluding here: `rimmerge.json` itself is
    // identical too, `generatedAt` never having existed to vary.
    assert_eq!(first, second);

    let json_file = first
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("rimmerge.json"))
        .unwrap();
    let FileContent::Text(text) = &json_file.content else {
        panic!("rimmerge.json must be text")
    };
    assert!(!text.contains("generatedAt"));
}

/// `Defs/` files land before `Patches/`
/// ones (matching the game's own `Defs/` before `Patches/` load
/// order), and an assignment render's `rimmerge.json` parses back
/// through the analyzer's own marker reader as `Assignment`.
#[test]
fn an_assignment_render_places_defs_before_patches_and_its_marker_round_trips() {
    let identity = patch_identity();
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    let scope: BTreeSet<ModId> = [
        ModId::new("example.bionicsfork"),
        ModId::new("example.racegroups"),
    ]
    .into_iter()
    .collect();
    let defs = vec![RenderedDefsFile {
        relative_path: defs_file_path("example.PartAssignmentDef"),
        content: "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<Defs>\n</Defs>\n".to_string(),
    }];
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "sample",
            description: "An assignment export.",
            dependencies: Dependencies::Exactly(&scope),
        },
        game_version: "1.6",
        plans: &plans,
        defs: &defs,
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::Assignment {
            assignment_id: "example-race-groups",
            profile_hash: "3f9a1c2b7d5e",
            scope: &scope,
            content_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };

    let rendered = render(&input).unwrap();

    let defs_index = rendered
        .files
        .iter()
        .position(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartAssignmentDef.xml"))
        .expect("Defs/ file must be present");
    let patches_index = rendered
        .files
        .iter()
        .position(|f| f.relative_path == Path::new("Patches/rimmerge_HediffDef.xml"))
        .expect("Patches/ file must be present");
    assert!(
        defs_index < patches_index,
        "Defs/ files must be rendered before Patches/ files"
    );

    let json_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("rimmerge.json"))
        .unwrap();
    let FileContent::Text(text) = &json_file.content else {
        panic!("rimmerge.json must be text")
    };
    assert!(text.contains(r#""kind":"assignment""#));
    assert!(text.contains(r#""assignmentId":"example-race-groups""#));

    let marker = rim_analyzer::extract::rimmerge_marker::parse(text.as_bytes())
        .expect("must parse as a marker");
    assert_eq!(marker.kind, rim_analyzer::domain::GeneratedKind::Assignment);
    assert_eq!(marker.patch_id.as_deref(), Some("example-race-groups"));
    assert_eq!(marker.scope, Some(scope));
}

/// An assignment export's `About.xml`
/// is rendered the same way a compat patch's is (`Dependencies::Exactly`
/// declares precisely the given scope, nothing more).
#[test]
fn about_xml_for_an_assignment_export_declares_precisely_the_scope() {
    let identity = GeneratedModIdentity {
        package_id: ModId::new("sample.partassign"),
        folder_name: "sample_partassign".to_string(),
        display_name: "Example Race Assignments".to_string(),
    };
    let plans = vec![worked_example_plan()];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.bionicsfork"),
        "Example Bionics Fork".to_string(),
    );
    mod_names.insert(
        ModId::new("example.framework"),
        "Example Framework".to_string(),
    );
    let scope: BTreeSet<ModId> = [
        ModId::new("example.bionicsfork"),
        ModId::new("example.framework"),
    ]
    .into_iter()
    .collect();
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "sample",
            description: "An assignment export.",
            dependencies: Dependencies::Exactly(&scope),
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::Assignment {
            assignment_id: "example-race-groups",
            profile_hash: "3f9a1c2b7d5e",
            scope: &scope,
            content_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };

    let rendered = render(&input).unwrap();
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .unwrap();
    let FileContent::Text(text) = &about.content else {
        panic!("About.xml must be text")
    };

    assert!(text.contains("sample.partassign"));
    assert!(text.contains("example.bionicsfork"));
    // `example.framework` contributes no op at all here, but `Exactly`
    // declares it anyway — the whole point of the mode, and exactly
    // how a real assignment export's framework dependency (never
    // touched by any `MergePlan`) still ends up declared.
    assert!(text.contains("example.framework"));
    assert!(!text.contains("ludeon.rimworld"));
}

/// `ExactlyWithLoadAfter`'s
/// `load_after_only` (T, a row's target — never a hard dependency)
/// lands in `loadAfter` but never in `modDependencies`, while
/// `depends_on` (R) lands in both — the shape a target-only mod with
/// no dependency on it still needs a load-order constraint relative to
/// this export's own mod.
#[test]
fn about_xml_with_exactly_load_after_declares_load_after_only_members_in_load_after_alone() {
    let identity = GeneratedModIdentity {
        package_id: ModId::new("sample.partassign"),
        folder_name: "sample_partassign".to_string(),
        display_name: "Example Race Assignments".to_string(),
    };
    let plans: Vec<MergePlan> = vec![];
    let mut mod_names = Map::new();
    mod_names.insert(
        ModId::new("example.framework"),
        "Example Framework".to_string(),
    );
    let depends_on: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let load_after_only: BTreeSet<ModId> = [ModId::new("target.races")].into_iter().collect();
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "sample",
            description: "An assignment export.",
            dependencies: Dependencies::ExactlyWithLoadAfter {
                depends_on: &depends_on,
                load_after_only: &load_after_only,
            },
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::Assignment {
            assignment_id: "example-race-groups",
            profile_hash: "3f9a1c2b7d5e",
            scope: &depends_on,
            content_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };

    let rendered = render(&input).unwrap();
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .unwrap();
    let FileContent::Text(text) = &about.content else {
        panic!("About.xml must be text")
    };

    let deps_section =
        &text[text.find("<modDependencies>").unwrap()..text.find("</modDependencies>").unwrap()];
    let load_after_section =
        &text[text.find("<loadAfter>").unwrap()..text.find("</loadAfter>").unwrap()];
    assert_eq!(
        extract_tag_values(deps_section, "packageId"),
        vec!["example.framework"],
        "modDependencies must be exactly depends_on, never a load-after-only member: {deps_section}"
    );
    assert_eq!(
        extract_tag_values(load_after_section, "li"),
        vec!["example.framework", "target.races"],
        "loadAfter must be exactly depends_on \u{222a} load_after_only: {load_after_section}"
    );
}
