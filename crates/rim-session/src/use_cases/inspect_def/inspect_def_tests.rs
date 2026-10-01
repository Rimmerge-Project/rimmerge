//! Tests for the def inspector.

use std::collections::BTreeMap;

use rim_analyzer::domain::{DefEntry, TemplateEntry};
use rim_merge::tree::Content;
use rim_resolve::domain::{Action, Decision, OrderSource, PairRule, Rule, RuleOrigin};

use super::*;
use crate::ports::DefSourceError;
use crate::test_support::{
    InMemoryDefSourceReader, bionic_heart_fixture, bionic_heart_fixture_with_conflict, locator,
    session_with_sources, session_with_sources_and_mods,
};
use rim_analyzer::domain::{FindModGate, PatchOp};
use rim_merge::effective::EffectiveDef;

fn bionic_heart_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        Selector::DefName,
    )
}

/// Independently rebuilds the `BionicHeart` fixture's own
/// `EffectiveInput` straight from its raw XML constants (never reading
/// anything `InspectDef::build` itself produced) and runs
/// `effective::compute` on it — the fixture has no patch contributions
/// at all, so `context` is a plausible stand-in never actually
/// consulted by the patch-replay stage.
fn independent_bionic_heart_effective(winner: &ModId) -> EffectiveDef {
    let core = ModId::new("ludeon.rimworld");
    let bionics = ModId::new("example.bionicsfork");
    let raw_text = if *winner == bionics {
        crate::test_support::BIONICS_BIONIC_HEART_XML
    } else {
        crate::test_support::CORE_BIONIC_HEART_XML
    };
    let raw = rim_merge::xml::parse(raw_text).expect("fixture XML must parse");

    let mut templates = BTreeMap::new();
    templates.insert(
        ("HediffDef".to_string(), "ImplantHediffBase".to_string()),
        rim_merge::xml::parse(crate::test_support::IMPLANT_HEDIFF_BASE_XML)
            .expect("fixture XML must parse"),
    );
    templates.insert(
        ("HediffDef".to_string(), "AddedBodyPartBase".to_string()),
        rim_merge::xml::parse(crate::test_support::ADDED_BODY_PART_BASE_XML)
            .expect("fixture XML must parse"),
    );
    templates.insert(
        ("HediffDef".to_string(), "addedPartExampleSynth".to_string()),
        rim_merge::xml::parse(crate::test_support::ADDED_PART_SYNTHETIC_XML)
            .expect("fixture XML must parse"),
    );
    let template_set = rim_merge::inherit::TemplateSet::new(templates);
    let mut template_owners = BTreeMap::new();
    template_owners.insert(
        ("HediffDef".to_string(), "ImplantHediffBase".to_string()),
        core.clone(),
    );
    template_owners.insert(
        ("HediffDef".to_string(), "AddedBodyPartBase".to_string()),
        core.clone(),
    );
    template_owners.insert(
        ("HediffDef".to_string(), "addedPartExampleSynth".to_string()),
        bionics.clone(),
    );

    let active_mods: BTreeSet<ModId> = [core, bionics].into_iter().collect();
    let mod_names_by_display: BTreeMap<String, ModId> = BTreeMap::new();
    let def_exists = |_: &str, _: &str| None;
    let context = ReplayContext {
        active_mods: &active_mods,
        mod_names_by_display: &mod_names_by_display,
        def_type: "HediffDef",
        def_name: "BionicHeart",
        selector: Selector::DefName,
        def_exists: &def_exists,
        this_def_present: true,
        behaviours: rim_merge::patch_behaviours::PatchOperationBehaviours::none(),
    };

    effective::compute(EffectiveInput {
        winner,
        raw,
        contributions: &[],
        context,
        templates: &template_set,
        template_owners: &template_owners,
    })
}

/// The `BionicHeart` fixture under both orders (a `PairRule` forces
/// `Suggested` to flip `ludeon.rimworld`/`example.bionicsfork` relative to
/// `Current`, since a plain `Source::Local` "ludeon.rimworld" fixture
/// mod carries no special sorter tier — see
/// `crates/rim-resolve/src/sort/tiers.rs`'s own `Source::Core`-only
/// gate): owners/winner flip with the order, and the effective def
/// this use case computes is `assert_eq!`-identical, field for field,
/// to an independent, direct call into `rim_merge::effective::compute`
/// fed the exact same raw text/templates
/// (`independent_bionic_heart_effective`) — not merely spot-checked
/// on one field.
#[test]
fn bionic_heart_inspection_flips_owners_and_winner_with_the_order() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("ludeon.rimworld"),
        before: ModId::new("example.bionicsfork"),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: false,
    }));
    let use_case = InspectDef::new(fixture.reader);
    let def_ref = bionic_heart_ref();

    session.select(OrderSource::Current);
    let current = use_case
        .execute(&mut session, &def_ref)
        .expect("Current must resolve")
        .clone();
    session.select(OrderSource::Suggested);
    let suggested = use_case
        .execute(&mut session, &def_ref)
        .expect("Suggested must resolve")
        .clone();

    assert_eq!(current.winner, ModId::new("example.bionicsfork"));
    assert_eq!(suggested.winner, ModId::new("ludeon.rimworld"));
    assert_eq!(
        current
            .owners
            .iter()
            .map(|t| t.mod_id.clone())
            .collect::<Vec<_>>(),
        vec![
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork")
        ]
    );
    assert_eq!(
        suggested
            .owners
            .iter()
            .map(|t| t.mod_id.clone())
            .collect::<Vec<_>>(),
        vec![
            ModId::new("example.bionicsfork"),
            ModId::new("ludeon.rimworld")
        ],
        "the pair rule must flip the suggested order's owner list too"
    );

    // `label` is a plain leaf each owner sets to a different value —
    // must always be `Owner(winner)`, tracking the flip.
    let label_path: rim_merge::tree::FieldPath = "label".parse().expect("valid field path");
    assert_eq!(
        current.effective.provenance.get(&label_path),
        Some(&effective::Provenance::Owner(ModId::new(
            "example.bionicsfork"
        )))
    );
    assert_eq!(
        suggested.effective.provenance.get(&label_path),
        Some(&effective::Provenance::Owner(ModId::new("ludeon.rimworld")))
    );

    // The real, field-for-field comparison this test's own doc
    // comment describes: an independent `effective::compute` call,
    // built straight from the fixture's own raw XML constants,
    // agrees with `InspectDef`'s own computed `effective` in full —
    // not just on `label`.
    assert_eq!(
        current.effective,
        independent_bionic_heart_effective(&current.winner)
    );
    assert_eq!(
        suggested.effective,
        independent_bionic_heart_effective(&suggested.winner)
    );

    // No patch ops were seeded for this def in either fixture order —
    // `bionic_heart_fixture` never populates `patch_ops_by_def`.
    assert!(current.patchers.is_empty());
    assert!(suggested.patchers.is_empty());
}

/// A def with one owner and two patchers targeting the same field —
/// both top-level operations are listed, one carrying a
/// `PatchOperationFindMod` gate, and the later-loaded patcher's value
/// wins in the effective def.
fn two_patcher_wall_fixture() -> (
    rim_analyzer::analysis::SourceIndex,
    rim_analyzer::domain::Report,
    InMemoryDefSourceReader,
) {
    let core = ModId::new("core.mod");
    let b = ModId::new("b.mod");
    let c = ModId::new("c.mod");
    let core_locator = locator("core_wall.xml", 0);
    let b_locator = locator("b_patch.xml", 0);
    let c_locator = locator("c_patch.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Wall".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![core.clone()],
    );

    let b_op = IndexedPatchOp {
        mod_id: b.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#.to_string()),
            target: None,
            find_mod_context: vec![FindModGate::AnyActive(vec!["Some Framework".to_string()])],
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: b_locator.clone(),
        },
    };
    let c_op = IndexedPatchOp {
        mod_id: c.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: c_locator.clone(),
        },
    };
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![b_op, c_op],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
            "<ThingDef><defName>Wall</defName><statBases><MaxHitPoints>200</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>250</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>300</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("ThingDef", "Wall", &["b.mod", "c.mod"])
        .build();

    (sources, report, InMemoryDefSourceReader::new(elements))
}

fn wall_ref() -> DefRef {
    DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        Selector::DefName,
    )
}

#[test]
fn a_two_patcher_def_lists_both_ops_with_gates_and_the_later_patcher_wins() {
    let (sources, report, reader) = two_patcher_wall_fixture();
    let mut session =
        session_with_sources_and_mods(sources, report, &["core.mod", "b.mod", "c.mod"]);
    let use_case = InspectDef::new(reader);

    let inspection = use_case
        .execute(&mut session, &wall_ref())
        .expect("must resolve");

    assert_eq!(inspection.patchers.len(), 2);
    let b = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == ModId::new("b.mod"))
        .expect("b.mod must be a patcher");
    assert_eq!(b.ops.len(), 1);
    assert_eq!(b.ops[0].class, "PatchOperationReplace");
    assert_eq!(
        b.ops[0].find_mod_context,
        vec![FindModGate::AnyActive(vec!["Some Framework".to_string()])]
    );
    assert!(b.replay.is_ok());
    let c = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == ModId::new("c.mod"))
        .expect("c.mod must be a patcher");
    assert_eq!(c.ops.len(), 1);
    assert!(c.ops[0].find_mod_context.is_empty());
    assert!(c.replay.is_ok());

    let path: rim_merge::tree::FieldPath =
        "statBases/MaxHitPoints".parse().expect("valid field path");
    assert_eq!(
        inspection.effective.provenance.get(&path),
        Some(&effective::Provenance::Patch {
            mod_id: ModId::new("c.mod"),
            op_index: 1,
        }),
        "c.mod loads after b.mod and replaces the same field, so it wins"
    );
}

/// Backs `inspect_defs_patchers_agree_with_plan_merges_own_contributions`:
/// three patchers of `ThingDef/Wall`. `b.mod` is a plain top-level
/// `PatchOperationReplace` on `statBases/MaxHitPoints`. `c.mod`
/// targets the same field, but its only mutating op is nested inside
/// a `PatchOperationSequence` wrapper — `patch_ops_by_def`'s own
/// indexed entry has `element_path == [0, 1]` (the wrapper's own
/// top-level ordinal, then the nested op's), exactly the shape
/// `SourceIndex::patch_ops_by_def` gives a wrapped op (it never
/// indexes the non-mutating wrapper itself) — proving
/// `top_level_operations`/`representative_op` neither double-count it
/// as two ops nor lose its identity. `d.mod` patches a third,
/// disjoint field (`description`) and is a genuine active patcher in
/// `patch_ops_by_def`, but is **not** named by the seeded
/// `PatchCollision` finding's own `mods` set (only `b.mod`/`c.mod`
/// collide) — proving `InspectDef`'s patcher list is the def's full,
/// unscoped participant set, not a copy of whichever finding happens
/// to exist for it.
fn three_patcher_wall_fixture() -> (
    rim_analyzer::analysis::SourceIndex,
    rim_analyzer::domain::Report,
    InMemoryDefSourceReader,
) {
    let core = ModId::new("core.mod");
    let b = ModId::new("b.mod");
    let c = ModId::new("c.mod");
    let d = ModId::new("d.mod");
    let core_locator = locator("core_wall.xml", 0);
    let b_locator = locator("b_patch.xml", 0);
    let c_wrapper_locator = locator("c_patch.xml", 0);
    let c_nested_locator = XmlLocator::new(c_wrapper_locator.file.clone(), vec![0, 1]);
    let d_locator = locator("d_patch.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Wall".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![core.clone()],
    );

    let make_replace_op = |mod_id: &ModId, xpath: &str, locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(xpath.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator,
        },
    };
    let b_op = make_replace_op(
        &b,
        r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
        b_locator.clone(),
    );
    let c_op = make_replace_op(
        &c,
        r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
        c_nested_locator,
    );
    let d_op = make_replace_op(
        &d,
        r#"Defs/ThingDef[defName="Wall"]/description"#,
        d_locator.clone(),
    );
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![b_op, c_op, d_op],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
            "<ThingDef><defName>Wall</defName><statBases><MaxHitPoints>200</MaxHitPoints></statBases><description>base</description></ThingDef>"
                .to_string());
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>250</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_wrapper_locator,
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                     <value><MaxHitPoints>300</MaxHitPoints></value>
                   </li>
                 </operations>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        d_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/description</xpath>
                 <value><description>d changed it</description></value>
               </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .mod_("d.mod")
        .patch_collision("ThingDef", "Wall", &["b.mod", "c.mod"])
        .build();

    (sources, report, InMemoryDefSourceReader::new(elements))
}

/// The same def also backs a real `PatchCollision` finding naming
/// only `b.mod`/`c.mod` — `InspectDef`'s own patcher list must still
/// agree with `PlanMerge`'s scoped preview on *those two* (same mods,
/// same order), while also proving two things a same-mods-same-order
/// check alone never could: a wrapped op is neither double-counted
/// nor mis-attributed, and `InspectDef` lists `d.mod` — a genuine
/// patcher `PlanMerge`'s own preview never sees because the finding
/// key it was built for doesn't name it.
#[test]
fn inspect_defs_patchers_agree_with_plan_merges_own_contributions() {
    let (sources, report, reader) = three_patcher_wall_fixture();
    let reader = std::sync::Arc::new(reader);
    let mut session =
        session_with_sources_and_mods(sources, report, &["core.mod", "b.mod", "c.mod", "d.mod"]);

    let plan = super::super::plan_merge::PlanMerge::new(reader.clone());
    let collision_key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: None,
        mods: [ModId::new("b.mod"), ModId::new("c.mod")]
            .into_iter()
            .collect(),
    };
    let preview = plan
        .execute(&mut session, &collision_key)
        .expect("planning must succeed")
        .clone();

    let inspect = InspectDef::new(reader);
    let inspection = inspect
        .execute(&mut session, &wall_ref())
        .expect("inspecting must succeed");

    assert_eq!(
        preview.owners,
        vec![ModId::new("b.mod"), ModId::new("c.mod")],
        "PlanMerge stays scoped to the finding key's own mods set"
    );
    assert_eq!(
        inspection
            .patchers
            .iter()
            .map(|p| p.mod_id.clone())
            .collect::<Vec<_>>(),
        vec![
            ModId::new("b.mod"),
            ModId::new("c.mod"),
            ModId::new("d.mod")
        ],
        "InspectDef's own patcher list is the def's full, unscoped \
             participant set — it must still list d.mod even though no \
             finding names it alongside b.mod/c.mod"
    );

    let c = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == ModId::new("c.mod"))
        .expect("c.mod must be a patcher");
    assert_eq!(
        c.ops.len(),
        1,
        "c.mod's only mutating op is nested inside a PatchOperationSequence \
             wrapper — it must be counted once, not lost or duplicated"
    );
    assert_eq!(c.ops[0].class, "PatchOperationReplace");
    assert!(
        c.ops[0].is_wrapped,
        "c.mod's op is a stand-in descendant reached past the \
             PatchOperationSequence wrapper, not the top-level node itself"
    );
    let b = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == ModId::new("b.mod"))
        .expect("b.mod must be a patcher");
    assert!(
        !b.ops[0].is_wrapped,
        "b.mod's op is a plain top-level PatchOperationReplace"
    );

    let path: rim_merge::tree::FieldPath =
        "statBases/MaxHitPoints".parse().expect("valid field path");
    assert_eq!(
        inspection.effective.provenance.get(&path),
        Some(&effective::Provenance::Patch {
            mod_id: ModId::new("c.mod"),
            op_index: 1,
        }),
        "c.mod's wrapped op still replays and wins the field, crediting c.mod"
    );
}

/// A `Name`-attributed template ref with no known `def_type`
/// ([`DefRef::name_only`]) resolves by scanning every def type's
/// templates, and lists its direct child.
#[test]
fn a_name_only_template_ref_resolves_and_lists_its_direct_child() {
    let owner = ModId::new("owner.mod");
    let child = ModId::new("child.mod");
    let template_locator = locator("owner_base_template.xml", 0);
    let child_locator = locator("child_reinforced.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            owner.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "ThingDef".to_string(),
                name: "Base".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: template_locator.clone(),
            },
        )],
    );
    sources.defs.insert(
        (
            child.clone(),
            ("ThingDef".to_string(), "Reinforced".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Reinforced".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("Base".to_string()),
            locator: child_locator,
        }],
    );
    sources.children_by_template.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            child.clone(),
            ("ThingDef".to_string(), "Reinforced".to_string()),
        )],
    );
    // `Reinforced` is a concrete def (it has its own `defName`), so
    // it must also be an `owners_by_def` entry — exactly what
    // `child_ref` checks to type this child `Selector::DefName`
    // rather than `NameAttr`.
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Reinforced".to_string()),
        vec![child.clone()],
    );
    let mut elements = BTreeMap::new();
    elements.insert(template_locator,
            "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("child.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["owner.mod", "child.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::name_only("Base");

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("a name-only ref must resolve across def types");

    assert_eq!(inspection.def_ref, def_ref, "echoed back verbatim");
    assert_eq!(inspection.winner, owner);
    assert_eq!(
        inspection.owners,
        vec![Toucher {
            mod_id: owner,
            position: 0,
            is_generated: false,
        }]
    );
    assert_eq!(
        inspection.children,
        vec![(
            child,
            DefRef::new(
                DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Reinforced".to_string(),
                },
                Selector::DefName
            )
        )],
        "Reinforced has its own defName, so it's a concrete-def child, not a NameAttr one"
    );
    assert!(inspection.parents.is_empty(), "Base has no ParentName");
}

/// A template whose own child is itself another template with no
/// `defName` of its own — a common real Core shape: `children` must
/// type such a
/// child `Selector::NameAttr`, not `DefName` (which would make it an
/// unresolvable, dead link), and that child ref must itself resolve
/// through `InspectDef`.
#[test]
fn a_templates_child_that_is_itself_a_template_yields_a_name_attr_ref_that_resolves() {
    let owner = ModId::new("owner.mod");
    let mid = ModId::new("mid.mod");
    let base_locator = locator("owner_base_template.xml", 0);
    let mid_locator = locator("mid_base_template.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            owner.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "ThingDef".to_string(),
                name: "Base".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: base_locator.clone(),
            },
        )],
    );
    sources.templates.insert(
        ("ThingDef".to_string(), "MidBase".to_string()),
        vec![(
            mid.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "ThingDef".to_string(),
                name: "MidBase".to_string(),
                parent_name: Some("Base".to_string()),
                is_abstract: true,
                locator: mid_locator.clone(),
            },
        )],
    );
    sources.children_by_template.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(mid.clone(), ("ThingDef".to_string(), "MidBase".to_string()))],
    );
    // `MidBase` is deliberately absent from `owners_by_def`: it has
    // no `defName` of its own, only a `Name` — the whole point of
    // this fixture.
    let mut elements = BTreeMap::new();
    elements.insert(base_locator,
            "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    elements.insert(
        mid_locator,
        "<ThingDef Name=\"MidBase\" ParentName=\"Base\"></ThingDef>".to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("mid.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["owner.mod", "mid.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let base_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Base".to_string(),
        },
        Selector::NameAttr,
    );

    let inspection = use_case
        .execute(&mut session, &base_ref)
        .expect("Base must resolve")
        .clone();

    let expected_child_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "MidBase".to_string(),
        },
        Selector::NameAttr,
    );
    assert_eq!(
        inspection.children,
        vec![(mid.clone(), expected_child_ref.clone())],
        "MidBase has no defName of its own, so it must be typed NameAttr, not DefName"
    );

    let mid_inspection = use_case
        .execute(&mut session, &expected_child_ref)
        .expect("the NameAttr child ref itself must resolve through InspectDef");
    assert_eq!(mid_inspection.winner, mid);
}

/// A [`Selector::DefName`] ref naming a def that exists only as an
/// abstract, `Name`-only template (no `owners_by_def` entry at all)
/// — `resolve_target`'s own DefName-miss fallback to the `templates`
/// map must still resolve it, as a
/// `NameAttr` target, rather than `NotFound`.
#[test]
fn a_def_name_ref_to_an_abstract_template_still_resolves() {
    let owner = ModId::new("owner.mod");
    let base_locator = locator("owner_base_template.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            owner.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "ThingDef".to_string(),
                name: "Base".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: base_locator.clone(),
            },
        )],
    );
    // No `owners_by_def` entry at all — `Base` is never a concrete
    // def in this fixture, only a template.
    let mut elements = BTreeMap::new();
    elements.insert(base_locator,
            "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["owner.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_name_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Base".to_string(),
        },
        Selector::DefName,
    );

    let inspection = use_case
        .execute(&mut session, &def_name_ref)
        .expect("a DefName ref must fall back to the templates map when no concrete def exists");

    assert_eq!(inspection.winner, owner);
    assert_eq!(
        inspection.def_ref, def_name_ref,
        "echoed back verbatim, still typed DefName even though it resolved via templates"
    );
}

/// An ordinary, typed `Selector::NameAttr` ref (a real `def_type`
/// known) with *two* registrants and no concrete child in hand — this
/// is `InspectDef`'s own always-abstract case
/// (`def_owner_and_raw`'s `NameAttr` branch never has a child to ask
/// with; see that function's own doc comment). There is no
/// registration-time winner at all — ground-truthed against the
/// decompiled engine, resolution is per *child*, and `InspectDef` has
/// none here — so "the first registrant wins" would be wrong. The
/// representative this crate reports with no child in hand is the
/// *last*-loaded registrant (`nearest_registration`'s own
/// `Asking::Nobody` arm — the widest single group any one representative
/// can honestly stand in for), disclosed via
/// `inspection.template_ambiguity` rather than presented as *the*
/// resolution.
#[test]
fn a_typed_name_attr_ref_with_two_registrants_and_no_child_in_hand_reports_the_last_loaded_as_a_disclosed_representative()
 {
    let first = ModId::new("first.mod");
    let second = ModId::new("second.mod");
    let first_locator = locator("first_base.xml", 0);
    let second_locator = locator("second_base.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![
            (
                first.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: first_locator.clone(),
                },
            ),
            (
                second.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: second_locator.clone(),
                },
            ),
        ],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        first_locator,
        "<ThingDef Name=\"Base\"><label>first</label></ThingDef>".to_string(),
    );
    elements.insert(
        second_locator,
        "<ThingDef Name=\"Base\"><label>second</label></ThingDef>".to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("first.mod")
        .mod_("second.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["first.mod", "second.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Base".to_string(),
        },
        Selector::NameAttr,
    );

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("must resolve");

    assert_eq!(
        inspection.winner, second,
        "no child is asking, so the disclosed representative is the last-loaded registrant"
    );
    assert_eq!(
        inspection
            .owners
            .iter()
            .map(|t| t.mod_id.clone())
            .collect::<Vec<_>>(),
        vec![first.clone(), second.clone()],
        "both registrants are still listed as owners, in load order"
    );
    let ambiguity = inspection
        .template_ambiguity
        .as_ref()
        .expect("two registrants with no child in hand must disclose the ambiguity");
    assert_eq!(ambiguity.registrants, vec![first, second]);
}

/// Nearest-registrant template resolution, proven where it actually matters: a
/// *concrete* child (`ThingDef/Wall`, `ParentName="Base"`) asking for
/// a template with **three** registrants — `r_early`/`r_mid`/`r_late`,
/// loading before/between/after the child. The three candidate
/// answers a naive implementation could produce are all distinct
/// here on purpose: "earliest registrant" (a wrong rule)
/// would say `r_early`; "last-loaded" (this crate's own disclosed
/// `Asking::Nobody` representative, if wrongly applied to a *known*
/// child too) would say `r_late`; only the real
/// `GetBestParentFor` rule — nearest *at or before* the child's own
/// position — says `r_mid`, since `r_late` loads after the child and
/// so is invisible to it. Distinguished by real content (each
/// registrant's own `<label>`), not just by owner id.
#[test]
fn a_concrete_childs_parent_name_resolves_to_the_nearest_registrant_at_or_before_it_not_the_earliest_or_the_last()
 {
    let early = ModId::new("r_early.mod");
    let mid = ModId::new("r_mid.mod");
    let late = ModId::new("r_late.mod");
    let child = ModId::new("child.mod");
    let early_locator = locator("early_base.xml", 0);
    let mid_locator = locator("mid_base.xml", 0);
    let late_locator = locator("late_base.xml", 0);
    let child_locator = locator("child_wall.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![
            (
                early.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: early_locator.clone(),
                },
            ),
            (
                mid.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: mid_locator.clone(),
                },
            ),
            (
                late.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: late_locator.clone(),
                },
            ),
        ],
    );
    sources.defs.insert(
        (child.clone(), ("ThingDef".to_string(), "Wall".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("Base".to_string()),
            locator: child_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![child.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        early_locator,
        "<ThingDef Name=\"Base\"><label>early</label></ThingDef>".to_string(),
    );
    elements.insert(
        mid_locator,
        "<ThingDef Name=\"Base\"><label>mid</label></ThingDef>".to_string(),
    );
    elements.insert(
        late_locator,
        "<ThingDef Name=\"Base\"><label>late</label></ThingDef>".to_string(),
    );
    elements.insert(
        child_locator,
        "<ThingDef ParentName=\"Base\"><defName>Wall</defName></ThingDef>".to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("r_early.mod")
        .mod_("r_mid.mod")
        .mod_("child.mod")
        .mod_("r_late.mod")
        .build();
    let mut session = session_with_sources_and_mods(
        sources,
        report,
        &["r_early.mod", "r_mid.mod", "child.mod", "r_late.mod"],
    );
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        Selector::DefName,
    );

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("must resolve");

    assert_eq!(
        inspection.parents,
        vec![(
            DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Base".to_string(),
            },
            mid.clone()
        )],
        "Wall's own ParentName=\"Base\" must resolve to r_mid (nearest at or before Wall's own position), not r_early (the old A1 rule) or r_late (loads after Wall, invisible to it)"
    );
    let Content::Children(root_children) = &inspection.effective.resolved.root.content else {
        panic!("Wall's own resolved root must have children (defName plus the inherited label)");
    };
    let label = root_children
        .iter()
        .find(|node| node.tag == "label")
        .expect("label must be present, inherited from Base");
    assert_eq!(
        label.content,
        Content::Text("mid".to_string()),
        "the effective label must come from r_mid's own copy specifically"
    );
}

/// A name-only ref (`DefRef::name_only`) resolving across *two* different
/// def types
/// sharing one `Name`: the "effective" `def_type` is whichever
/// entry's earliest active owner sits soonest **overall** in the
/// selected order — not whichever def type happens to sort first
/// alphabetically. A third entry, registered by the *same* mod as one
/// of the first two (a genuine position tie), documents the
/// tie-break: `resolve_name_only_def_type`'s own `min_by_key` keeps
/// the first-encountered entry among ties, and `SourceIndex::templates`
/// is a `BTreeMap` keyed `(def_type, Name)`, so that's whichever tied
/// `def_type` sorts alphabetically first.
#[test]
fn a_name_only_ref_picks_the_earliest_owner_overall_and_ties_break_alphabetically() {
    let first = ModId::new("first.mod");
    let second = ModId::new("second.mod");
    let zeta_locator = locator("zeta_shared.xml", 0);
    let alpha_locator = locator("alpha_shared.xml", 0);
    let beta_locator = locator("beta_shared.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    // "Zeta" (alphabetically last) is registered by `first.mod`,
    // loaded *before* `second.mod` — it must still win over "Alpha"
    // (alphabetically first, but registered by the later-loading
    // `second.mod`): position in the order decides, not spelling.
    sources.templates.insert(
        ("Zeta".to_string(), "Shared".to_string()),
        vec![(
            first.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "Zeta".to_string(),
                name: "Shared".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: zeta_locator.clone(),
            },
        )],
    );
    sources.templates.insert(
        ("Alpha".to_string(), "Shared".to_string()),
        vec![(
            second.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "Alpha".to_string(),
                name: "Shared".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: alpha_locator.clone(),
            },
        )],
    );
    // "Beta" is registered by `first.mod` too — a genuine tie with
    // "Zeta" (same earliest-owner position). Alphabetically "Beta" <
    // "Zeta", and that's the documented tie-break.
    sources.templates.insert(
        ("Beta".to_string(), "Shared".to_string()),
        vec![(
            first.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "Beta".to_string(),
                name: "Shared".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: beta_locator.clone(),
            },
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        zeta_locator,
        "<Zeta Name=\"Shared\"><label>zeta</label></Zeta>".to_string(),
    );
    elements.insert(
        alpha_locator,
        "<Alpha Name=\"Shared\"><label>alpha</label></Alpha>".to_string(),
    );
    elements.insert(
        beta_locator,
        "<Beta Name=\"Shared\"><label>beta</label></Beta>".to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("first.mod")
        .mod_("second.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["first.mod", "second.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::name_only("Shared");

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("a name-only ref must resolve across def types");

    assert_eq!(
        inspection.winner, first,
        "Beta ties with Zeta on position (both first.mod), but wins the \
             tie-break alphabetically; either way first.mod (not second.mod) \
             is the effective owner"
    );
    assert_eq!(
        inspection
            .owners
            .iter()
            .map(|t| t.mod_id.clone())
            .collect::<Vec<_>>(),
        vec![first, second],
        "owners are merged across every def type sharing the Name"
    );
}

/// A real `PatchOperationFindMod` wrapper naming a mod that isn't
/// active: the wrapped mutating op never runs at all — no
/// `<nomatch>` block means `patch_eval`'s own `apply_branch` returns
/// cleanly with no change and no caveat, exactly like the real game —
/// so the patcher contributes nothing to `effective`, and its own
/// `replay`/`caveats` show a clean, empty pass, not a gap.
#[test]
fn a_find_mod_wrapper_naming_an_inactive_mod_gates_the_op_off() {
    let core = ModId::new("core.mod");
    let b = ModId::new("b.mod");
    let core_locator = locator("core_widget.xml", 0);
    let b_wrapper_locator = locator("b_patch.xml", 0);
    let b_nested_locator = XmlLocator::new(b_wrapper_locator.file.clone(), vec![0, 1]);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Widget".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Widget".to_string()),
        vec![core.clone()],
    );
    let b_op = IndexedPatchOp {
        mod_id: b.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints"#.to_string()),
            target: None,
            find_mod_context: vec![FindModGate::AnyActive(vec![
                "Some Inactive Framework".to_string(),
            ])],
            find_mod_names: vec!["Some Inactive Framework".to_string()],
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: b_nested_locator,
        },
    };
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![b_op],
    );
    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
            "<ThingDef><defName>Widget</defName><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    // "Some Inactive Framework" names no active mod's display name
    // below, so this gate never opens — and there is no `<nomatch>`
    // branch, so the whole op is silently skipped.
    elements.insert(
        b_wrapper_locator,
        r#"<Operation Class="PatchOperationFindMod">
                 <mods><li>Some Inactive Framework</li></mods>
                 <match Class="PatchOperationReplace">
                   <xpath>Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints</xpath>
                   <value><MaxHitPoints>999</MaxHitPoints></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("b.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["core.mod", "b.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    );

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("a gated-off op must not fail the whole inspection");

    assert_eq!(
        inspection.effective.completeness,
        effective::Completeness::Complete
    );
    let patcher = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == b)
        .expect("b.mod must still be listed as a patcher");
    assert_eq!(
        patcher.ops[0].find_mod_context,
        vec![FindModGate::AnyActive(vec![
            "Some Inactive Framework".to_string()
        ])]
    );
    assert_eq!(patcher.replay, Ok(()));
    assert!(patcher.reached);
    assert!(
        patcher.caveats.is_empty(),
        "a cleanly gated-off op is not a caveat, it's simply a no-op: {:?}",
        patcher.caveats
    );
    let path: rim_merge::tree::FieldPath =
        "statBases/MaxHitPoints".parse().expect("valid field path");
    assert_eq!(
        inspection.effective.provenance.get(&path),
        Some(&effective::Provenance::Owner(core)),
        "the gated-off op never touched the field — it stays credited to the owner"
    );
}

/// A generated mod (the profile merge mod, or an exported compat
/// patch) is flagged `is_generated`, never hidden, whether it's a
/// def's *owner* or one of its *patchers* — built via
/// `test_support::session_with_sources_and_generated` so the
/// generated marker actually lands on a real, indexed owner/patcher
/// `InspectDef` can see (`session_fixture_with_generated`'s own
/// `SourceIndex::default()` has nothing to inspect).
#[test]
fn a_generated_mod_is_flagged_whether_owner_or_patcher() {
    let core = ModId::new("core.mod");
    let gen_mod = ModId::new("generated.mod");
    let core_locator = locator("core_widget.xml", 0);
    let gen_owned_locator = locator("gen_widget.xml", 0);
    let gen_patch_locator = locator("gen_patch.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Widget".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Widget".to_string()),
        vec![core.clone()],
    );
    let gen_op = IndexedPatchOp {
        mod_id: gen_mod.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: gen_patch_locator.clone(),
        },
    };
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![gen_op],
    );
    // `generated.mod` also owns its own def, directly.
    sources.defs.insert(
        (
            gen_mod.clone(),
            ("ThingDef".to_string(), "GenWidget".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "GenWidget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: gen_owned_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "GenWidget".to_string()),
        vec![gen_mod.clone()],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
            "<ThingDef><defName>Widget</defName><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    elements.insert(
        gen_owned_locator,
        "<ThingDef><defName>GenWidget</defName></ThingDef>".to_string(),
    );
    elements.insert(
        gen_patch_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>200</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );

    let mut session = crate::test_support::session_with_sources_and_generated(
        sources,
        &["core.mod", "generated.mod"],
        "generated.mod",
    );
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));

    let widget_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    );
    let widget_inspection = use_case
        .execute(&mut session, &widget_ref)
        .expect("must resolve")
        .clone();
    let patcher = widget_inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == gen_mod)
        .expect("generated.mod must be a patcher");
    assert!(
        patcher.is_generated,
        "a generated patcher must be flagged, not hidden"
    );

    let gen_widget_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "GenWidget".to_string(),
        },
        Selector::DefName,
    );
    let gen_widget_inspection = use_case
        .execute(&mut session, &gen_widget_ref)
        .expect("must resolve");
    let owner = gen_widget_inspection
        .owners
        .iter()
        .find(|t| t.mod_id == gen_mod)
        .expect("generated.mod must be an owner");
    assert!(
        owner.is_generated,
        "a generated owner must be flagged, not hidden"
    );
}

/// A `ParentName` naming a template with no `SourceIndex::templates`
/// entry at all: `def_sources::template_chain` stops the walk
/// silently (`inspection.parents` stays empty — the walk never even
/// starts collecting) rather than erroring the whole inspection, and
/// `rim_merge::effective::compute`'s own internal `ParentName`
/// resolution discovers the identical gap on its own, surfacing it as
/// a *display* fact (`Completeness::Partial { stopped_at:
/// Stopper::Inherit(InheritError::MissingParent) }`), never a hard
/// `Err`.
#[test]
fn a_broken_parent_chain_stops_silently_and_surfaces_as_a_display_stopper() {
    let owner = ModId::new("owner.mod");
    let def_locator = locator("owner_widget.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (
            owner.clone(),
            ("ThingDef".to_string(), "Widget".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: Some("Ghost".to_string()),
            locator: def_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Widget".to_string()),
        vec![owner.clone()],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        def_locator,
        r#"<ThingDef ParentName="Ghost"><defName>Widget</defName><label>widget</label></ThingDef>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &["owner.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    );

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("a broken template chain must not fail the whole inspection");

    assert!(
        inspection.parents.is_empty(),
        "the walk stops before collecting the missing template"
    );
    assert!(
        matches!(&inspection.effective.completeness,
            effective::Completeness::Partial {
                stopped_at: effective::Stopper::Inherit(rim_merge::inherit::InheritError::MissingParent { name, .. }
                ),
            } if name == "Ghost"
        ),
        "{:?}",
        inspection.effective.completeness
    );
}

/// A top-level operation whose `Class` `patch_eval::replay` doesn't
/// recognize stops the full load-ordered fold: `DefInspection.effective`'s
/// `completeness` is `Partial { stopped_at: Stopper::Replay { .. } }`
/// naming the offending mod and its op index, never guessed past
/// ("never skip a stopper and continue"). [`Patcher::replay`] makes the identical, standalone
/// finding for that one mod's own ops.
#[test]
fn an_unsupported_op_stops_the_fold_and_surfaces_as_a_replay_stopper() {
    let core = ModId::new("core.mod");
    let a = ModId::new("a.mod");
    let b = ModId::new("b.mod");
    let e = ModId::new("e.mod");
    let core_locator = locator("core_widget.xml", 0);
    let a_locator = locator("a_patch.xml", 0);
    let b_locator = locator("b_patch.xml", 0);
    let e_locator = locator("e_patch.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Widget".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Widget".to_string()),
        vec![core.clone()],
    );
    // `a.mod` loads before the unsupported op: a `PatchOperationReplace`
    // targeting a field that doesn't exist matches nothing — the real
    // game logs it and moves on, and so does this replay
    // (`Caveat::FailedOp`, not an error) — its own contribution is
    // fully reached and counted as a success.
    let a_op = IndexedPatchOp {
        mod_id: a.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Widget"]/statBases/NoSuchField"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: a_locator.clone(),
        },
    };
    let b_op = IndexedPatchOp {
        mod_id: b.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "Some.Unknown.PatchOperationThing".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Widget"]/statBases"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: b_locator.clone(),
        },
    };
    // `e.mod` loads after the unsupported op — a perfectly ordinary
    // op the fold never gets to attempt.
    let e_op = IndexedPatchOp {
        mod_id: e.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: e_locator.clone(),
        },
    };
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![a_op, b_op, e_op],
    );
    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
            "<ThingDef><defName>Widget</defName><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Widget"]/statBases/NoSuchField</xpath>
                 <value><NoSuchField>1</NoSuchField></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="Some.Unknown.PatchOperationThing">
                 <xpath>Defs/ThingDef[defName="Widget"]/statBases</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="Widget"]/statBases</xpath>
                   <value><y>2</y></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        e_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Widget"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>999</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("e.mod")
        .build();
    let mut session =
        session_with_sources_and_mods(sources, report, &["core.mod", "a.mod", "b.mod", "e.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        Selector::DefName,
    );

    let inspection = use_case
        .execute(&mut session, &def_ref)
        .expect("an unsupported op must not fail the whole inspection");

    assert!(
        matches!(&inspection.effective.completeness,
            effective::Completeness::Partial {
                stopped_at: effective::Stopper::Replay { mod_id, op_index: 1, .. },
            } if *mod_id == b
        ),
        "a.mod's own op is index 0 and replays cleanly (if uselessly); \
             b.mod's op is index 1 and is the real stopper: {:?}",
        inspection.effective.completeness
    );

    let patcher_a = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == a)
        .expect("a.mod must be a patcher");
    assert_eq!(
        patcher_a.replay,
        Ok(()),
        "a.mod's own op replayed cleanly before the stopper"
    );
    assert!(patcher_a.reached, "a.mod's op was fully attempted");
    assert!(
        matches!(patcher_a.caveats.as_slice(),
            [Caveat::FailedOp { mod_id, .. }] if *mod_id == a
        ),
        "a zero-match Replace is a caveat, never an error: {:?}",
        patcher_a.caveats
    );

    let patcher_b = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == b)
        .expect("b.mod must be a patcher");
    assert!(
        patcher_b.replay.is_err(),
        "b.mod's own op is the fold's own stopper"
    );
    assert!(patcher_b.reached, "the fold did reach b.mod's own op");

    let patcher_e = inspection
        .patchers
        .iter()
        .find(|p| p.mod_id == e)
        .expect("e.mod must be a patcher");
    assert_eq!(
        patcher_e.replay,
        Ok(()),
        "no evidence of failure — e.mod's op was simply never attempted"
    );
    assert!(
        !patcher_e.reached,
        "e.mod's own op loads after the stopper and the fold never got to it"
    );
    assert!(patcher_e.caveats.is_empty());
}

/// A `FindingKey::DuplicateTemplateName`'s own `def_ref()` is always
/// name-only (`rim_analyzer`'s `Indices::template_owners` has no
/// `def_type` to give it) — inspecting the *same* template through an
/// ordinary, typed `Selector::NameAttr` ref must still surface it
///
#[test]
fn a_name_only_finding_surfaces_on_a_typed_ref_inspection_of_the_same_name() {
    let owner = ModId::new("owner.mod");
    let other = ModId::new("other.mod");
    let template_locator = locator("owner_base_template.xml", 0);

    let mut sources = rim_analyzer::analysis::SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![(
            owner.clone(),
            TemplateEntry {
                graphic_class: None,
                may_require: Vec::new(),
                def_type: "ThingDef".to_string(),
                name: "Base".to_string(),
                parent_name: None,
                is_abstract: true,
                locator: template_locator.clone(),
            },
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(template_locator,
            "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("other.mod")
        .build();
    // No builder helper exists for this conflict kind in
    // `rim-resolve`'s test support — pushed directly onto
    // the built `Report`, same shape `ReportBuilder::def_override`'s
    // own push uses internally.
    report
        .conflicts
        .push(rim_analyzer::domain::Conflict::DuplicateTemplateName(
            rim_analyzer::domain::DuplicateTemplateName {
                name: "Base".to_string(),
                owners: vec![owner.clone(), other.clone()],
            },
        ));
    let mut session = session_with_sources_and_mods(sources, report, &["owner.mod", "other.mod"]);
    let use_case = InspectDef::new(InMemoryDefSourceReader::new(elements));
    let typed_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Base".to_string(),
        },
        Selector::NameAttr,
    );

    let inspection = use_case
        .execute(&mut session, &typed_ref)
        .expect("must resolve");

    assert_eq!(
        inspection.findings.len(),
        1,
        "the DuplicateTemplateName finding must surface even though its \
             own def_ref() is name-only, not this typed ref"
    );
    assert!(
        matches!(&inspection.findings[0].0,
            FindingKey::DuplicateTemplateName { name, .. } if name == "Base"
        ),
        "{:?}",
        inspection.findings
    );
}

/// A locator whose seeded content no longer matches what it's
/// expected to be surfaces as `InspectDefError::Source(DefSourceError::Stale)`,
/// never a silent mis-read. Staled on the *winner*'s own file
/// (`example.bionicsfork`, the last owner in the default order) — unlike
/// `PlanMerge`, which diffs every owner's raw node, `InspectDef` only
/// ever reads the effective owner's own text directly, so staling a
/// non-winning owner's file (as `PlanMerge`'s own equivalent test
/// does) would never be reached here.
#[test]
fn a_stale_locator_surfaces_as_a_source_error() {
    let mut fixture = bionic_heart_fixture();
    let winner_locator = locator("bionics_bionic.xml", 0);
    fixture.reader = fixture.reader.with_error(
        winner_locator.clone(),
        DefSourceError::Stale {
            file: winner_locator.file.to_path_buf(),
            path: winner_locator.element_path.clone(),
            expected: "HediffDef/BionicHeart".to_string(),
        },
    );
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = InspectDef::new(fixture.reader);

    let result = use_case.execute(&mut session, &bionic_heart_ref());

    assert!(matches!(
        result,
        Err(InspectDefError::Source(DefSourceError::Stale { .. }))
    ));
}

/// A ref naming nothing this scan indexed is `NotFound`, never a
/// panic or a guessed answer.
#[test]
fn an_unknown_ref_is_not_found() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = InspectDef::new(fixture.reader);
    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "DoesNotExist".to_string(),
        },
        Selector::DefName,
    );

    let result = use_case.execute(&mut session, &def_ref);

    assert_eq!(result, Err(InspectDefError::NotFound(def_ref)));
}

/// An inspection is cached per `(source, def_ref)`: it survives a
/// decision that leaves `sorter_overrides()` unchanged (which only
/// calls `invalidate_ledgers`, never touching `orders`), but any rule
/// change — which always recomputes the sort — drops it, since an
/// inspection's owners/winner/effective depend on the order.
///
/// The cache survives the decision, but `DefInspection.findings` must
/// not: it's a live view of the ledger's `ResolutionStatus`, and a
/// non-resorting decision on this very def's own finding changes that
/// status without invalidating anything else the cached inspection
/// holds. A cache-hit that just replays the stale `findings` it was
/// built with would keep reporting the pre-decision status forever.
#[test]
fn an_inspection_survives_a_non_resorting_decision_but_not_a_resort() {
    let fixture = bionic_heart_fixture_with_conflict();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = InspectDef::new(fixture.reader);
    let def_ref = bionic_heart_ref();
    let finding_key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    };

    let before = use_case
        .execute(&mut session, &def_ref)
        .expect("must resolve");
    assert_eq!(before.findings.len(), 1, "the seeded DefOverride finding");
    assert_ne!(
        before.findings[0].1,
        ResolutionStatus::UserOverridden,
        "nothing has been decided yet"
    );
    assert!(session.inspection(OrderSource::Current, &def_ref).is_some());

    // `Accept`/`RemoveMod` are the two actions
    // `DecisionSet::sorter_overrides` leaves untouched (see
    // `rim_resolve::domain::decision`'s own
    // `sorter_overrides_ignores_accept_and_remove_mod` test) — this
    // decision only calls `Session::invalidate_ledgers`, never
    // `Session::recompute_sort`. It targets the inspected def's own
    // finding directly, so its `ResolutionStatus` flips to
    // `UserOverridden` without the cache entry itself being cleared.
    session
        .decide(Decision {
            key: finding_key,
            action: Action::Accept,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("accept is always a valid action");
    assert!(
        session.inspection(OrderSource::Current, &def_ref).is_some(),
        "a decision that doesn't change the sorter's overrides must not drop the cache"
    );

    let after = use_case
        .execute(&mut session, &def_ref)
        .expect("must still resolve from cache");
    assert_eq!(after.findings.len(), 1);
    assert_eq!(
        after.findings[0].1,
        ResolutionStatus::UserOverridden,
        "a cache hit must report the finding's *current* status, not the one \
             it was cached with"
    );

    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("ludeon.rimworld"),
        before: ModId::new("example.bionicsfork"),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: false,
    }));
    assert!(
        session.inspection(OrderSource::Current, &def_ref).is_none(),
        "any rule change recomputes the sort and must drop the cache"
    );
}

/// A cache entry is scoped to its own `(source, def_ref)` slot —
/// computing an inspection under `Current` leaves `Suggested`'s own
/// cache empty, exactly like `PlanMerge`'s own per-slot preview cache.
#[test]
fn an_inspection_is_scoped_to_its_own_order_source() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = InspectDef::new(fixture.reader);
    let def_ref = bionic_heart_ref();

    use_case
        .execute(&mut session, &def_ref)
        .expect("Current must resolve");

    assert!(session.inspection(OrderSource::Current, &def_ref).is_some());
    assert!(
        session
            .inspection(OrderSource::Suggested, &def_ref)
            .is_none(),
        "an inspection computed under Current must not leak into Suggested's cache"
    );
}

/// The `Session.inspections` cache is bounded — once caching a new
/// entry would push it past its cap, the whole cache clears first
/// rather than growing without bound across a session that inspects
/// many different defs. Exercised directly against `Session::cache_inspection`
/// (rather than through 257 real `InspectDef::execute` calls, which
/// would each need their own indexed def) with a minimal hand-built
/// `DefInspection` per synthetic `DefRef` — this test only cares
/// about the cache's own eviction policy, not what a real inspection
/// contains.
#[test]
fn the_inspections_cache_clears_itself_once_it_would_exceed_its_cap() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let trivial_tree = rim_merge::xml::parse("<a></a>").expect("trivial XML must parse");
    let dummy_inspection = |def_ref: DefRef| DefInspection {
        def_ref,
        source: OrderSource::Current,
        owners: Vec::new(),
        winner: ModId::new("ludeon.rimworld"),
        template_ambiguity: None,
        patchers: Vec::new(),
        parents: Vec::new(),
        children: Vec::new(),
        effective: EffectiveDef {
            resolved: trivial_tree.clone(),
            provenance: BTreeMap::new(),
            completeness: effective::Completeness::Complete,
            caveats: Vec::new(),
            top_level_outcomes: Vec::new(),
            suppressed_filter_head_ops: 0,
        },
        resolved_xml: String::new(),
        findings: Vec::new(),
    };
    let synthetic_ref = |i: u32| {
        DefRef::new(
            DefKey {
                def_type: "ThingDef".to_string(),
                def_name: format!("Dummy{i}"),
            },
            Selector::DefName,
        )
    };

    let first_ref = synthetic_ref(0);
    session.cache_inspection(
        OrderSource::Current,
        first_ref.clone(),
        dummy_inspection(first_ref.clone()),
    );
    for i in 1..256 {
        let def_ref = synthetic_ref(i);
        session.cache_inspection(
            OrderSource::Current,
            def_ref.clone(),
            dummy_inspection(def_ref),
        );
    }
    assert!(
        session
            .inspection(OrderSource::Current, &first_ref)
            .is_some(),
        "256 entries total must not yet exceed the cap"
    );

    let overflow_ref = synthetic_ref(256);
    session.cache_inspection(
        OrderSource::Current,
        overflow_ref.clone(),
        dummy_inspection(overflow_ref.clone()),
    );

    assert!(
        session
            .inspection(OrderSource::Current, &first_ref)
            .is_none(),
        "the 257th distinct entry must have cleared the whole cache first"
    );
    assert!(
        session
            .inspection(OrderSource::Current, &overflow_ref)
            .is_some(),
        "the entry that triggered the clear must still end up cached"
    );
}
