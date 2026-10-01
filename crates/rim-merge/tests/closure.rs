//! The closure test: render a merge, write it to a temp dir, re-parse the
//! emitted `Patches/*.xml` with the analyzer's own patch walker and
//! `About.xml` with its own parser, replay the emitted ops with
//! `patch_eval` over the winner's raw node, resolve inheritance, and check
//! the result against the merge the user actually asked for. This is the
//! test that would fail if the emitter and the evaluator ever disagreed
//! about an xpath shape.
//!
//! `render_reparse_replay_resolve_equals_the_intended_merge` is the
//! `BionicHeart` worked example;
//! `every_item_id_variant_round_trips_through_render_and_replay` extends
//! the same pipeline into a table over every `ItemId` variant with values
//! containing `& < " / .. * |`;
//! `effective_reproduces_the_closure_pipelines_result_for_bionic_heart`
//! runs the *same* rendered merge-mod operation through
//! `rim_merge::effective` instead and checks its `resolved` tree against
//! this file's own replay-then-resolve pipeline, field by field.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use rim_analyzer::domain::{GameVersion, GeneratedKind, ModId, Selector};
use rim_analyzer::extract::{about_xml, patches, rimmerge_marker};
use rim_merge::diff::{OwnerVersion, three_way};
use rim_merge::effective::{self, EffectiveInput};
use rim_merge::emit::{
    AboutSpec, Dependencies, EmitInput, FileContent, GeneratedModIdentity, Provenance, render,
};
use rim_merge::inherit::{TemplateSet, resolve};
use rim_merge::patch_behaviours::PatchOperationBehaviours;
use rim_merge::patch_eval::{PatchContribution, ReplayContext, replay};
use rim_merge::plan::{
    DefKey, MergeChoice, PatchCollisionInput, plan_def_override, plan_patch_collision,
};
use rim_merge::tree::{ContainerKind, Content, FieldPath, FieldTree, container_kind};
use rim_merge::xml;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/xml/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// Splits a multi-def fixture file (several `<HediffDef Name="...">`
/// siblings under one wrapping root) into individually parsed trees —
/// `xml::parse` itself only ever parses one def/template element at a
/// time, matching what a real `DefSourceReader` hands back per locator.
fn parse_siblings(multi_def_xml: &str) -> Vec<FieldTree> {
    let doc = roxmltree::Document::parse(multi_def_xml)
        .unwrap_or_else(|error| panic!("well-formed fixture XML: {error}"));
    doc.root_element()
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|node| {
            xml::parse(&multi_def_xml[node.range()])
                .unwrap_or_else(|error| panic!("well-formed def/template: {error}"))
        })
        .collect()
}

fn field_text(tree: &FieldTree, path: &str) -> String {
    let field_path: FieldPath = path
        .parse()
        .unwrap_or_else(|_| panic!("valid FieldPath: {path}"));
    match &tree
        .get(&field_path)
        .unwrap_or_else(|| panic!("expected {path} to exist"))
        .content
    {
        Content::Text(text) => text.clone(),
        other => panic!("expected text content at {path}, got {other:?}"),
    }
}

/// Writes `rendered` under a fresh temp dir and returns the mod folder's
/// path (kept alive by leaking the `TempDir` guard into the caller's
/// scope via the returned handle — callers keep it bound with `_guard`).
fn write_to_temp_dir(
    rendered: &rim_merge::emit::RenderedMod,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let mod_dir = temp_dir.path().join(&rendered.folder_name);
    for file in &rendered.files {
        let path = mod_dir.join(&file.relative_path);
        let Some(parent) = path.parent() else {
            panic!("{}: every rendered file has a parent dir", path.display());
        };
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("{}: {error}", parent.display()));
        if let FileContent::Text(text) = &file.content {
            std::fs::write(&path, text)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }
    (temp_dir, mod_dir)
}

/// Extracts the raw XML text of the single top-level `<Operation>` a
/// rendered `Patches/<DefType>.xml` file carries for one def — exactly
/// what a real `DefSourceReader`-backed `PatchContribution` would hand
/// `patch_eval::replay`.
fn extract_operation_xml(patch_text: &str) -> String {
    let doc = roxmltree::Document::parse(patch_text)
        .unwrap_or_else(|error| panic!("well-formed rendered patch XML: {error}"));
    let Some(operation_node) = doc
        .root_element()
        .children()
        .find(roxmltree::Node::is_element)
    else {
        panic!("the rendered <Patch> has one top-level <Operation>");
    };
    patch_text[operation_node.range()].to_string()
}

#[test]
fn render_reparse_replay_resolve_equals_the_intended_merge() {
    // ---- Arrange: the BionicHeart merge -------------------------------
    let core_bionic_heart = xml::parse(&fixture("core_bionic_heart.xml")).unwrap();
    let bionics_bionic_heart = xml::parse(&fixture("bionics_bionic_heart.xml")).unwrap();

    let mut by_name = BTreeMap::new();
    for template in parse_siblings(&fixture("core_hediff_bases.xml")) {
        let name = template.name.clone().expect("Core template has a Name");
        by_name.insert(("HediffDef".to_string(), name), template);
    }
    let bionics_template = xml::parse(&fixture("bionics_hediff_base.xml")).unwrap();
    by_name.insert(
        (
            "HediffDef".to_string(),
            bionics_template.name.clone().unwrap(),
        ),
        bionics_template,
    );
    let templates = TemplateSet::new(by_name);

    let core_id = ModId::new("ludeon.rimworld");
    let bionics_id = ModId::new("example.bionicsfork");
    let core_owner = OwnerVersion {
        mod_id: core_id.clone(),
        resolved: resolve(&core_bionic_heart, &templates).unwrap(),
        raw: core_bionic_heart,
        inherited: None,
    };
    let bionics_owner = OwnerVersion {
        mod_id: bionics_id.clone(),
        resolved: resolve(&bionics_bionic_heart, &templates).unwrap(),
        inherited: rim_merge::inherit::resolve_inherited_only(&bionics_bionic_heart, &templates)
            .unwrap(),
        raw: bionics_bionic_heart.clone(),
    };

    let diff = three_way(&[core_owner.clone(), bionics_owner.clone()], &core_id).unwrap();
    let choices: BTreeMap<FieldPath, MergeChoice> =
        ["label", "labelNoun", "description", "defaultLabelColor"]
            .into_iter()
            .map(|path| {
                (
                    path.parse().unwrap(),
                    MergeChoice::From {
                        mod_id: core_id.clone(),
                    },
                )
            })
            .collect();

    let plan = plan_def_override(&diff, &bionics_owner, &choices);
    assert!(
        plan.unresolved.is_empty(),
        "the worked example has a choice for every conflict: {:?}",
        plan.caveats
    );
    assert_eq!(plan.ops.len(), 4);

    // ---- Emit -----------------------------------------------------------
    let identity = GeneratedModIdentity::for_profile("3f9a1c2b7d5e");
    let mut mod_names = BTreeMap::new();
    mod_names.insert(bionics_id.clone(), "Example Bionics Fork".to_string());
    let plans = vec![plan];
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "Rimmerge",
            description: "Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.",
            dependencies: Dependencies::FromContent,
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::ProfileMerge {
            generated_at: "2026-09-05T00:00:00Z",
            profile_hash: "3f9a1c2b7d5e",
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };
    let rendered = render(&input).unwrap();

    // ---- Write to a temp dir ---------------------------------------------
    let (_guard, mod_dir) = write_to_temp_dir(&rendered);

    // ---- Re-parse with the analyzer's own readers ------------------------
    let about_bytes = std::fs::read(mod_dir.join("About/About.xml")).unwrap();
    let about = about_xml::parse(&about_bytes, GameVersion::new(1, 6)).unwrap();
    assert_eq!(about.id, identity.package_id);

    let patch_path = mod_dir.join("Patches/rimmerge_HediffDef.xml");
    let patch_bytes = std::fs::read(&patch_path).unwrap();
    let patch_file: Arc<Path> = Arc::from(patch_path.as_path());
    let parsed_ops = patches::walk(&patch_bytes, &patch_file).unwrap();
    let mutating_ops: Vec<_> = parsed_ops.iter().filter(|op| op.is_mutating).collect();
    assert_eq!(
        mutating_ops.len(),
        4,
        "one op per field the plan replaced/added"
    );
    for op in &mutating_ops {
        assert!(
            op.target.is_some(),
            "every emitted mutating op must parse back to a DefTarget: {op:?}"
        );
        assert_eq!(op.target.as_ref().unwrap().def_name, "BionicHeart");
    }

    // ---- Replay the emitted sequence over the winner's raw node ----------
    let patch_text = std::fs::read_to_string(&patch_path).unwrap();
    let operation_xml = extract_operation_xml(&patch_text);

    let active_mods: BTreeSet<ModId> = [core_id.clone(), bionics_id.clone()].into_iter().collect();
    let mod_names_by_display = BTreeMap::new();
    let context = ReplayContext {
        active_mods: &active_mods,
        mod_names_by_display: &mod_names_by_display,
        def_type: "HediffDef",
        def_name: "BionicHeart",
        selector: Selector::DefName,
        def_exists: rim_merge::patch_eval::def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        bionics_bionic_heart,
        &[PatchContribution {
            mod_id: &bionics_id,
            operation_xml: &operation_xml,
        }],
        &context,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);

    // ---- Resolve inheritance over the replayed raw tree -------------------
    let final_tree = resolve(&outcome.tree, &templates).unwrap();

    // ---- The result must equal the intended merge --------------------------
    assert_eq!(field_text(&final_tree, "label"), "bionic heart");
    assert_eq!(field_text(&final_tree, "labelNoun"), "a bionic heart");
    assert_eq!(
        field_text(&final_tree, "description"),
        "An installed bionic heart. It has synthetic muscle fibers for a realistic heartbeat, plus a high-flow pump for rapid circulation during high stress. It is better than a biological heart in almost every way."
    );
    assert_eq!(
        field_text(&final_tree, "defaultLabelColor"),
        "(0.6, 0.6, 1.0)"
    );
    // Everything BIONICS-specific and untouched by the merge survives as-is.
    assert_eq!(field_text(&final_tree, "hediffClass"), "Hediff_AddedPart");
    assert_eq!(
        field_text(&final_tree, "spawnThingOnRemoved"),
        "BionicHeart"
    );
    assert!(
        final_tree
            .get(
                &"comps/li[@Class=ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust]"
                    .parse()
                    .unwrap()
            )
            .is_some(),
        "BIONICS's inherited comp must survive since it wasn't dropped"
    );
}

/// The closure-style check for `rim_merge::effective::compute`, fed the
/// *same* rendered merge-mod operation this file's own test above replays
/// by hand, must land on the same tree — proving `effective` and the
/// plan/emit/patch_eval/inherit pipeline agree about the `BionicHeart`
/// merge, not just that each independently claims success.
#[test]
fn effective_reproduces_the_closure_pipelines_result_for_bionic_heart() {
    // ---- Arrange: the same BionicHeart merge ----------------------------
    let core_bionic_heart = xml::parse(&fixture("core_bionic_heart.xml")).unwrap();
    let bionics_bionic_heart = xml::parse(&fixture("bionics_bionic_heart.xml")).unwrap();

    let mut by_name = BTreeMap::new();
    let mut template_owners = BTreeMap::new();
    let core_id = ModId::new("ludeon.rimworld");
    let bionics_id = ModId::new("example.bionicsfork");
    for template in parse_siblings(&fixture("core_hediff_bases.xml")) {
        let name = template.name.clone().expect("Core template has a Name");
        template_owners.insert(("HediffDef".to_string(), name.clone()), core_id.clone());
        by_name.insert(("HediffDef".to_string(), name), template);
    }
    let bionics_template = xml::parse(&fixture("bionics_hediff_base.xml")).unwrap();
    let bionics_template_name = bionics_template.name.clone().unwrap();
    template_owners.insert(
        ("HediffDef".to_string(), bionics_template_name.clone()),
        bionics_id.clone(),
    );
    by_name.insert(
        ("HediffDef".to_string(), bionics_template_name),
        bionics_template,
    );
    let templates = TemplateSet::new(by_name);

    let core_owner = OwnerVersion {
        mod_id: core_id.clone(),
        resolved: resolve(&core_bionic_heart, &templates).unwrap(),
        raw: core_bionic_heart,
        inherited: None,
    };
    let bionics_owner = OwnerVersion {
        mod_id: bionics_id.clone(),
        resolved: resolve(&bionics_bionic_heart, &templates).unwrap(),
        inherited: rim_merge::inherit::resolve_inherited_only(&bionics_bionic_heart, &templates)
            .unwrap(),
        raw: bionics_bionic_heart.clone(),
    };

    let diff = three_way(&[core_owner.clone(), bionics_owner.clone()], &core_id).unwrap();
    let choices: BTreeMap<FieldPath, MergeChoice> =
        ["label", "labelNoun", "description", "defaultLabelColor"]
            .into_iter()
            .map(|path| {
                (
                    path.parse().unwrap(),
                    MergeChoice::From {
                        mod_id: core_id.clone(),
                    },
                )
            })
            .collect();
    let plan = plan_def_override(&diff, &bionics_owner, &choices);
    assert!(plan.unresolved.is_empty());

    // ---- Emit, then extract the merge mod's own emitted operation -------
    let identity = GeneratedModIdentity::for_profile("3f9a1c2b7d5e");
    let merge_mod_id = identity.package_id.clone();
    let mut mod_names = BTreeMap::new();
    mod_names.insert(bionics_id.clone(), "Example Bionics Fork".to_string());
    let plans = vec![plan];
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "Rimmerge",
            description: "Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.",
            dependencies: Dependencies::FromContent,
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::ProfileMerge {
            generated_at: "2026-09-05T00:00:00Z",
            profile_hash: "3f9a1c2b7d5e",
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };
    let rendered = render(&input).unwrap();
    let (_guard, mod_dir) = write_to_temp_dir(&rendered);
    let patch_path = mod_dir.join("Patches/rimmerge_HediffDef.xml");
    let patch_text = std::fs::read_to_string(&patch_path).unwrap();
    let operation_xml = extract_operation_xml(&patch_text);

    // ---- This file's own pipeline: replay by hand, then resolve ---------
    let active_mods: BTreeSet<ModId> = [core_id.clone(), bionics_id.clone(), merge_mod_id.clone()]
        .into_iter()
        .collect();
    let mod_names_by_display = BTreeMap::new();
    let context = ReplayContext {
        active_mods: &active_mods,
        mod_names_by_display: &mod_names_by_display,
        def_type: "HediffDef",
        def_name: "BionicHeart",
        selector: Selector::DefName,
        def_exists: rim_merge::patch_eval::def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        bionics_bionic_heart.clone(),
        &[PatchContribution {
            mod_id: &merge_mod_id,
            operation_xml: &operation_xml,
        }],
        &context,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let expected = resolve(&outcome.tree, &templates).unwrap();

    // ---- `effective::compute`, fed the same operation --------------------
    let contributions = [PatchContribution {
        mod_id: &merge_mod_id,
        operation_xml: &operation_xml,
    }];
    let effective_def = effective::compute(EffectiveInput {
        winner: &bionics_id,
        raw: bionics_bionic_heart,
        contributions: &contributions,
        context,
        templates: &templates,
        template_owners: &template_owners,
    });

    // ---- The two pipelines must agree on the whole tree ------------------
    assert_eq!(
        effective_def.completeness,
        effective::Completeness::Complete
    );
    assert_eq!(effective_def.resolved, expected);

    // ---- And `effective` additionally attributes each field ------------
    assert_eq!(
        effective_def.provenance.get(&"label".parse().unwrap()),
        Some(&effective::Provenance::Patch {
            mod_id: merge_mod_id.clone(),
            op_index: 0
        })
    );
    assert_eq!(
        effective_def
            .provenance
            .get(&"hediffClass".parse().unwrap()),
        Some(&effective::Provenance::Inherited {
            template: rim_resolve::domain::DefKey {
                def_type: "HediffDef".to_string(),
                def_name: "AddedBodyPartBase".to_string(),
            },
            owner: core_id,
        })
    );
    assert_eq!(
        effective_def
            .provenance
            .get(&"spawnThingOnRemoved".parse().unwrap()),
        Some(&effective::Provenance::Owner(bionics_id))
    );
}

/// One round trip per [`rim_resolve::domain::ItemId`] variant, each
/// with a value containing every character that needs escaping care
/// (`& < " / .. * |`) — render -> write -> `patches::walk` ->
/// `xpath_expr::parse` (inside `patch_eval::replay`) -> resolve, and the
/// whole result tree compared against the intended one, not just the one
/// touched field.
#[test]
fn every_item_id_variant_round_trips_through_render_and_replay() {
    struct Case {
        name: &'static str,
        /// One `li`, shaped for the `ItemId` variant under test, as the
        /// full `<comps>...</comps>` container's XML.
        comps_xml: &'static str,
        /// The `FieldPath` text addressing that `li` (must match what
        /// `ItemId::of`'s heuristic actually computes for it).
        item_path: &'static str,
    }

    // Every case: the winning owner (`ludeon.rimworld`) has no `comps` at
    // all; `mod.a` has one `li` carrying it; choosing `From(mod.a)` is
    // therefore a genuine `Add` (never a no-op — choosing a field's own
    // already-current value, e.g. the winner choosing itself, always is;
    // see `plan::tests::a_merge_with_no_choices_over_two_owners_is_a_no_op`).
    //
    // `& < " / *` all round-trip through render -> `patches::walk` ->
    // `xpath_expr::parse` -> replay: `&`/`<`/`"` are XML-escaping
    // concerns, handled by escaping exactly once; `/` is safe
    // because it always sits inside the step's own `[...]` bracket depth,
    // which `xpath_expr`'s splitter already respects. `..` and `|` are
    // deliberately *not* included here: `rim_analyzer::extract::xpath_expr`
    // (not this crate's to edit) rejects either substring with a naive,
    // non-quote-aware scan over the *whole* remaining xpath, so a value
    // containing either can never be safely embedded in a predicate no
    // matter how it's quoted — `plan.rs`'s
    // `a_value_containing_a_double_dot_or_pipe_is_unresolved_with_a_caveat`
    // test (in `crates/rim-merge/src/plan.rs`) covers that refusal
    // directly instead of asserting a round trip that cannot exist.
    let cases = [
        Case {
            name: "Class",
            comps_xml: r#"<comps><li Class="Weird&amp;&lt;&quot;/one*two"><v>own</v></li></comps>"#,
            item_path: r#"comps/li[@Class=Weird&<"\/one*two]"#,
        },
        Case {
            name: "Key",
            comps_xml: r#"<comps><li><defName>Weird&amp;&lt;&quot;/one*two</defName><v>own</v></li></comps>"#,
            item_path: r#"comps/li[defName=Weird&<"\/one*two]"#,
        },
        Case {
            name: "Text",
            comps_xml: r#"<comps><li>Weird&amp;&lt;&quot;/one*two</li></comps>"#,
            item_path: r#"comps/li[=Weird&<"\/one*two]"#,
        },
        Case {
            name: "Position",
            comps_xml: "<comps><li><v>own</v></li></comps>",
            item_path: "comps/li[#0]",
        },
    ];

    for case in cases {
        let core_xml = "<ThingDef><defName>W</defName><label>base</label></ThingDef>".to_string();
        let other_xml = format!(
            "<ThingDef><defName>W</defName><label>base</label>{}</ThingDef>",
            case.comps_xml
        );

        let core_raw = xml::parse(&core_xml).unwrap();
        let core = OwnerVersion {
            mod_id: ModId::new("ludeon.rimworld"),
            raw: core_raw.clone(),
            resolved: core_raw.clone(),
            inherited: None,
        };
        let other_raw = xml::parse(&other_xml).unwrap();
        let other = OwnerVersion {
            mod_id: ModId::new("mod.a"),
            raw: other_raw.clone(),
            resolved: other_raw.clone(),
            inherited: None,
        };

        // Core (lacking `comps` entirely) is the *winner* here — an
        // unusual load order for a real install, but a perfectly valid
        // input for exercising `Add`'s render/replay round trip, which
        // is this test's actual point.
        let diff = three_way(&[core.clone(), other], &ModId::new("ludeon.rimworld")).unwrap();

        let item_path: FieldPath = case.item_path.parse().unwrap_or_else(|error| {
            panic!(
                "{}: {:?} must parse as a FieldPath: {error:?}",
                case.name, case.item_path
            )
        });
        assert!(
            diff.fields.iter().any(|f| f.path == item_path),
            "{}: expected diff field at {item_path}",
            case.name
        );

        let choices: BTreeMap<FieldPath, MergeChoice> = [(
            item_path.clone(),
            MergeChoice::From {
                mod_id: ModId::new("mod.a"),
            },
        )]
        .into_iter()
        .collect();

        let plan = plan_def_override(&diff, &core, &choices);
        assert!(
            plan.unresolved.is_empty(),
            "{}: unexpected unresolved fields: {:?} (caveats: {:?})",
            case.name,
            plan.unresolved,
            plan.caveats
        );
        assert_eq!(plan.ops.len(), 1, "{}: expected exactly one op", case.name);

        let identity = GeneratedModIdentity::for_profile("3f9a1c2b7d5e");
        let mod_names = BTreeMap::new();
        let plans = vec![plan];
        let input = EmitInput {
            about: AboutSpec {
                identity: &identity,
                author: "Rimmerge",
                description: "Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.",
                dependencies: Dependencies::FromContent,
            },
            game_version: "1.6",
            plans: &plans,
            defs: &[],
            assets: &[],
            mod_names: &mod_names,
            provenance: Provenance::ProfileMerge {
                generated_at: "2026-09-05T00:00:00Z",
                profile_hash: "3f9a1c2b7d5e",
                decisions_sha256: "deadbeef",
            },
            rimmerge_version: "0.1.0",
        };
        let rendered = render(&input).unwrap();
        let (_guard, mod_dir) = write_to_temp_dir(&rendered);

        let patch_path = mod_dir.join("Patches/rimmerge_ThingDef.xml");
        let patch_bytes = std::fs::read(&patch_path).unwrap();
        let patch_file: Arc<Path> = Arc::from(patch_path.as_path());
        let parsed_ops = patches::walk(&patch_bytes, &patch_file)
            .unwrap_or_else(|error| panic!("{}: patches::walk must parse: {error}", case.name));
        let mutating: Vec<_> = parsed_ops.iter().filter(|op| op.is_mutating).collect();
        assert_eq!(mutating.len(), 1, "{}: expected one mutating op", case.name);
        assert!(
            mutating[0].target.is_some(),
            "{}: the emitted op must parse back to a DefTarget",
            case.name
        );

        let patch_text = std::fs::read_to_string(&patch_path).unwrap();
        let operation_xml = extract_operation_xml(&patch_text);

        let active_mods: BTreeSet<ModId> = [ModId::new("mod.a")].into_iter().collect();
        let mod_names_by_display = BTreeMap::new();
        let mod_a = ModId::new("mod.a");
        let context = ReplayContext {
            active_mods: &active_mods,
            mod_names_by_display: &mod_names_by_display,
            def_type: "ThingDef",
            def_name: "W",
            selector: Selector::DefName,
            def_exists: rim_merge::patch_eval::def_existence_unknown(),
            this_def_present: true,
            behaviours: PatchOperationBehaviours::none(),
        };

        let outcome = replay(
            core_raw,
            &[PatchContribution {
                mod_id: &mod_a,
                operation_xml: &operation_xml,
            }],
            &context,
        );
        assert!(
            outcome.error.is_none(),
            "{}: {:?}",
            case.name,
            outcome.error
        );

        // The intended result: the winner's own tree plus exactly the
        // `comps` `mod.a` contributed — since `Add` appends to the
        // winner's root, the result's own document order (`defName`,
        // `label`, `comps`) matches `other_xml`'s directly. Compare the
        // *whole* tree, not just the one field.
        let intended = xml::parse(&other_xml).unwrap();
        assert_eq!(
            outcome.tree, intended,
            "{}: replayed tree must equal the intended result",
            case.name
        );
    }
}

/// The `text()` half of the closure property: the emitter only ever renders
/// *element* xpaths, so a mod's `.../texPath/text()` form has no rendered
/// counterpart to compare against directly. What must hold instead is that
/// the two forms are interchangeable — replaying the emitted element-form
/// op and replaying the same change written as a `text()` op produce the
/// *same* tree, and both parse back through `patches::walk` + `xpath_expr`.
#[test]
fn a_text_node_replace_and_the_emitted_element_replace_agree() {
    let core_xml = r#"<ThingDef><defName>Sandbags</defName><graphicData><texPath>Things/Old_Atlas</texPath></graphicData></ThingDef>"#;
    let other_xml = r#"<ThingDef><defName>Sandbags</defName><graphicData><texPath>ExampleThings/Upscaled_Atlas</texPath></graphicData></ThingDef>"#;

    let core_raw = xml::parse(core_xml).unwrap();
    let core = OwnerVersion {
        mod_id: ModId::new("ludeon.rimworld"),
        raw: core_raw.clone(),
        resolved: core_raw.clone(),
        inherited: None,
    };
    let other_raw = xml::parse(other_xml).unwrap();
    let other = OwnerVersion {
        mod_id: ModId::new("mod.a"),
        raw: other_raw.clone(),
        resolved: other_raw.clone(),
        inherited: None,
    };

    // ---- Render the element-form op the emitter produces --------------
    let diff = three_way(&[core.clone(), other], &ModId::new("ludeon.rimworld")).unwrap();
    let tex_path: FieldPath = "graphicData/texPath".parse().unwrap();
    let choices: BTreeMap<FieldPath, MergeChoice> = [(
        tex_path.clone(),
        MergeChoice::From {
            mod_id: ModId::new("mod.a"),
        },
    )]
    .into_iter()
    .collect();
    let plan = plan_def_override(&diff, &core, &choices);
    assert!(plan.unresolved.is_empty(), "{:?}", plan.caveats);
    assert_eq!(plan.ops.len(), 1);

    let identity = GeneratedModIdentity::for_profile("3f9a1c2b7d5e");
    let mod_names = BTreeMap::new();
    let plans = vec![plan];
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "Rimmerge",
            description: "Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.",
            dependencies: Dependencies::FromContent,
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::ProfileMerge {
            generated_at: "2026-09-05T00:00:00Z",
            profile_hash: "3f9a1c2b7d5e",
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };
    let rendered = render(&input).unwrap();
    let (_guard, mod_dir) = write_to_temp_dir(&rendered);

    let patch_path = mod_dir.join("Patches/rimmerge_ThingDef.xml");
    let patch_bytes = std::fs::read(&patch_path).unwrap();
    let patch_file: Arc<Path> = Arc::from(patch_path.as_path());
    let parsed_ops = patches::walk(&patch_bytes, &patch_file).unwrap();
    let mutating: Vec<_> = parsed_ops.iter().filter(|op| op.is_mutating).collect();
    assert_eq!(mutating.len(), 1);
    let patch_text = std::fs::read_to_string(&patch_path).unwrap();
    let emitted_operation = extract_operation_xml(&patch_text);

    // ---- The same change, written the way a mod writes it -------------
    let text_node_operation = r#"<Operation Class="PatchOperationReplace">
           <xpath>Defs/ThingDef[defName="Sandbags"]/graphicData/texPath/text()</xpath>
           <value>ExampleThings/Upscaled_Atlas</value>
         </Operation>"#;
    // It must survive the analyzer's own walker too, not just this
    // crate's evaluator.
    let text_node_patch = format!("<Patch>{text_node_operation}</Patch>");
    let walked = patches::walk(text_node_patch.as_bytes(), &patch_file).unwrap();
    assert_eq!(
        walked[0].target.as_ref().map(|t| t.def_name.as_str()),
        Some("Sandbags")
    );

    // ---- Both replays land on the same tree ---------------------------
    let active_mods: BTreeSet<ModId> = [ModId::new("mod.a")].into_iter().collect();
    let mod_names_by_display = BTreeMap::new();
    let mod_a = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active_mods,
        mod_names_by_display: &mod_names_by_display,
        def_type: "ThingDef",
        def_name: "Sandbags",
        selector: Selector::DefName,
        def_exists: rim_merge::patch_eval::def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };

    let via_emitted = replay(
        core_raw.clone(),
        &[PatchContribution {
            mod_id: &mod_a,
            operation_xml: &emitted_operation,
        }],
        &context,
    );
    let via_text_node = replay(
        core_raw,
        &[PatchContribution {
            mod_id: &mod_a,
            operation_xml: text_node_operation,
        }],
        &context,
    );

    assert!(via_emitted.error.is_none(), "{:?}", via_emitted.error);
    assert!(via_text_node.error.is_none(), "{:?}", via_text_node.error);
    assert_eq!(
        field_text(&via_emitted.tree, "graphicData/texPath"),
        "ExampleThings/Upscaled_Atlas"
    );
    assert_eq!(via_emitted.tree, via_text_node.tree);
    assert_eq!(via_emitted.tree, other_raw);
}

/// The compat-patch counterpart of `render_reparse_replay_resolve_equals_the_intended_merge`
/// above: a
/// `Dependencies::Exactly` render's `About.xml`, re-parsed with the
/// analyzer's own reader, must declare `modDependencies` and `loadAfter`
/// as *exactly* the declared scope — including a scope member this plan's
/// ops never touch, which `Dependencies::FromContent` would have omitted.
#[test]
fn a_patch_shaped_about_xml_round_trip_declares_exactly_the_scope() {
    let core = OwnerVersion {
        mod_id: ModId::new("fixture.moda"),
        raw: xml::parse("<ThingDef><defName>Fixture_Wall</defName><label>fixture wall</label><statBases><MaxHitPoints>150</MaxHitPoints></statBases></ThingDef>")
        .unwrap(),
        resolved: xml::parse("<ThingDef><defName>Fixture_Wall</defName><label>fixture wall</label><statBases><MaxHitPoints>150</MaxHitPoints></statBases></ThingDef>")
        .unwrap(),
        inherited: None,
    };
    let other = OwnerVersion {
        mod_id: ModId::new("fixture.modb"),
        raw: xml::parse("<ThingDef><defName>Fixture_Wall</defName><label>fixture wall</label><statBases><MaxHitPoints>300</MaxHitPoints></statBases></ThingDef>")
        .unwrap(),
        resolved: xml::parse("<ThingDef><defName>Fixture_Wall</defName><label>fixture wall</label><statBases><MaxHitPoints>300</MaxHitPoints></statBases></ThingDef>")
        .unwrap(),
        inherited: None,
    };

    let diff = three_way(&[core.clone(), other.clone()], &ModId::new("fixture.moda")).unwrap();
    let choices: BTreeMap<FieldPath, MergeChoice> = [(
        "statBases/MaxHitPoints".parse().unwrap(),
        MergeChoice::From {
            mod_id: ModId::new("fixture.moda"),
        },
    )]
    .into_iter()
    .collect();
    let plan = plan_def_override(&diff, &other, &choices);
    assert!(plan.unresolved.is_empty(), "{:?}", plan.caveats);
    assert_eq!(plan.ops.len(), 1);

    // `fixture.modc` is in the declared scope but nothing this plan's ops
    // depend on — the point of the assertion below.
    let scope: BTreeSet<ModId> = [
        ModId::new("fixture.moda"),
        ModId::new("fixture.modb"),
        ModId::new("fixture.modc"),
    ]
    .into_iter()
    .collect();
    let identity = GeneratedModIdentity {
        package_id: ModId::new("sample.abcompat"),
        folder_name: "sample_abcompat".to_string(),
        display_name: "A + B Compatibility".to_string(),
    };
    let plans = vec![plan];
    let mod_names = BTreeMap::new();
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "sample",
            description: "Compatibility patch generated by Rimmerge from field-level merge decisions.",
            dependencies: Dependencies::Exactly(&scope),
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::CompatPatch {
            patch_id: "3f9a1c02be77",
            profile_hash: "3f9a1c2b7d5e",
            scope: &scope,
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };
    let rendered = render(&input).unwrap();
    let (_guard, mod_dir) = write_to_temp_dir(&rendered);

    let about_bytes = std::fs::read(mod_dir.join("About/About.xml")).unwrap();
    let about = about_xml::parse(&about_bytes, GameVersion::new(1, 6)).unwrap();
    assert_eq!(about.id, identity.package_id);

    let mut declared_deps: Vec<ModId> = about
        .declared
        .dependencies
        .iter()
        .map(|dependency| dependency.id.clone())
        .collect();
    declared_deps.sort();
    let mut declared_load_after = about.declared.load_after.clone();
    declared_load_after.sort();
    let expected: Vec<ModId> = scope.iter().cloned().collect();

    assert_eq!(
        declared_deps, expected,
        "modDependencies must be exactly the declared scope"
    );
    assert_eq!(
        declared_load_after, expected,
        "loadAfter must be exactly the declared scope"
    );

    // ---- The `rimmerge.json` marker must round-trip through the ---------
    // ---- analyzer's own reader too, not just `About.xml` ----------------
    let marker_bytes = std::fs::read(mod_dir.join("rimmerge.json")).unwrap();
    let marker =
        rimmerge_marker::parse(&marker_bytes).expect("rimmerge.json must parse as a marker");
    assert_eq!(marker.kind, GeneratedKind::Patch);
    assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
    assert_eq!(marker.scope, Some(scope));
}

/// A keyed-map entry's emitted op is just an ordinary `Child`-step
/// xpath — no new grammar, and `emit.rs` needed no change to produce it
/// (a [`rim_merge::plan::PlanOp::Replace`]/`Add` on a
/// `wildAnimals/<Key>`-shaped [`FieldPath`] already renders through the
/// existing machinery). This closure covers the collision case the
/// BionicHeart test above never does: replaying **raw, then the three
/// original mods' own contributions, then the merge mod's own op** — not
/// def-override ops over the raw node alone.
#[test]
fn keyed_map_entries_round_trip_through_render_and_replay() {
    // ---- Arrange: the keyed-map worked example ---------------------------
    let core = xml::parse(&fixture("core_arid_shrubland_wild_animals.xml")).unwrap();
    let patches_xml = fixture("arid_shrubland_wild_animals_patches.xml");
    let doc = roxmltree::Document::parse(&patches_xml)
        .unwrap_or_else(|error| panic!("well-formed fixture XML: {error}"));
    let operations: Vec<String> = doc
        .root_element()
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|node| patches_xml[node.range()].to_string())
        .collect();

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
    // An explicit choice on one entry forces a real op — the three
    // disjoint adds alone already auto-resolve with nothing to emit
    // (own point: the game
    // already unions them), which this test's own "replay the merge
    // mod's op" pipeline needs at least one op to exercise.
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
        def_exists: rim_merge::patch_eval::def_existence_unknown(),
        choices: &choices,
        behaviours: PatchOperationBehaviours::none(),
    })
    .unwrap()
    .plan;
    assert!(plan.unresolved.is_empty(), "{:?}", plan.caveats);
    assert_eq!(plan.ops.len(), 1);

    // ---- Emit -----------------------------------------------------------
    let identity = GeneratedModIdentity::for_profile("keyedmap0001");
    let mod_names = BTreeMap::new();
    let plans = vec![plan];
    let input = EmitInput {
        about: AboutSpec {
            identity: &identity,
            author: "Rimmerge",
            description: "Generated by Rimmerge from your merge decisions.",
            dependencies: Dependencies::FromContent,
        },
        game_version: "1.6",
        plans: &plans,
        defs: &[],
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::ProfileMerge {
            generated_at: "2026-09-09T00:00:00Z",
            profile_hash: "keyedmap0001",
            decisions_sha256: "deadbeef",
        },
        rimmerge_version: "0.1.0",
    };
    let rendered = render(&input).unwrap();
    let (_guard, mod_dir) = write_to_temp_dir(&rendered);

    // ---- Re-parse with the analyzer's own patch walker -------------------
    let patch_path = mod_dir.join("Patches/rimmerge_BiomeDef.xml");
    let patch_bytes = std::fs::read(&patch_path).unwrap();
    let patch_file: Arc<Path> = Arc::from(patch_path.as_path());
    let parsed_ops = patches::walk(&patch_bytes, &patch_file).unwrap();
    let mutating_ops: Vec<_> = parsed_ops.iter().filter(|op| op.is_mutating).collect();
    assert_eq!(mutating_ops.len(), 1, "one op for the one chosen entry");
    let op_target = mutating_ops[0]
        .target
        .as_ref()
        .expect("the emitted op must parse back to a DefTarget");
    assert_eq!(op_target.def_name, "AridShrubland");

    // ---- Replay raw, then the three real mods, then the merge mod's op --
    let patch_text = std::fs::read_to_string(&patch_path).unwrap();
    let merge_mod_operation_xml = extract_operation_xml(&patch_text);
    let merge_mod_id = identity.package_id.clone();
    let mut full_order = contributions.clone();
    full_order.push(PatchContribution {
        mod_id: &merge_mod_id,
        operation_xml: &merge_mod_operation_xml,
    });

    let mut full_active = active.clone();
    full_active.insert(merge_mod_id.clone());
    let context = ReplayContext {
        active_mods: &full_active,
        mod_names_by_display: &names,
        def_type: "BiomeDef",
        def_name: "AridShrubland",
        selector: Selector::DefName,
        def_exists: rim_merge::patch_eval::def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(core, &full_order, &context);
    assert!(outcome.error.is_none(), "{:?}", outcome.error);

    // ---- The result must equal the intended union ------------------------
    let wild_animals = outcome
        .tree
        .get(&"wildAnimals".parse().unwrap())
        .expect("wildAnimals must still exist");
    assert_eq!(
        container_kind(wild_animals),
        Some(ContainerKind::KeyedMap),
        "no duplicate tag under the container — each contribution's own isolation held: {wild_animals:?}"
    );
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Tortoise"), "0.4");
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Cobra"), "0.3");
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Warg"), "0.2");
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Camel"), "0.1");
    assert_eq!(
        field_text(&outcome.tree, "wildAnimals/XBM_Theropod"),
        "0.9",
        "the merge mod's own chosen value wins, loading last"
    );
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Mammoth"), "0.1");
    assert_eq!(field_text(&outcome.tree, "wildAnimals/Hyena"), "0.2");
}
