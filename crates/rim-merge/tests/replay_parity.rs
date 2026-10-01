//! **The test that is the whole point of keeping this table as data**:
//! replaying the same real operations with the *bundled* class map
//! produces exactly what this crate's own handlers give the invented
//! names those same behaviours are pinned under in `patch_eval_tests.rs`,
//! and replaying them with an *empty* map degrades to `Unsupported` —
//! never to some other behaviour, and never silently to a different tree.
//!
//! Two claims, pinned separately:
//!
//! 1. **"the bundled map reproduces this crate's own modelled
//!    behaviour"** — the expected trees below are exactly what
//!    `patch_eval.rs`'s own handlers produce; the real class names'
//!    mapping onto those handlers, via the bundled data, is what this
//!    file checks.
//! 2. **the degradation shape** — with no map loaded, each of the three
//!    classes is `ReplayError::Unsupported`, which is this crate's own
//!    "might be wrong, so refuse" contract, not a wrong answer.
//!
//! **This file reads `rules/rimmerge-rules.json` directly** — the same
//! built, embedded bundle `rim-io`'s `build.rs` compiles into the real
//! binary (see that file, and `crates/rim-io/src/mod_knowledge.rs`) —
//! rather than restating its `patch_operations` rows. Restating them
//! would let the two drift and still pass. `rim-merge` cannot depend on
//! `rim-io` (the crate graph runs the other way), and `rules/` is a git
//! submodule the whole workspace shares, so the file is read by relative
//! path and its rows are turned into a `PatchOperationBehaviours` here,
//! with `serde_json` as a dev-dependency only.
//!
//! **This file is allowed to name real third-party classes.** Nothing
//! under `crates/*/src` or `apps/*/src` may (`scripts/check-forbidden.ps1`
//! enforces that on the public tree); here, naming them is what the test
//! is for.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{ModId, Selector};
use rim_merge::patch_behaviours::{
    ClassBehaviour, ClassGate, ClassMatch, ConditionalKind, CustomBehaviour, GateBehaviour,
    GateFields, PatchOperationBehaviours,
};
use rim_merge::patch_eval::{
    PatchContribution, ReplayContext, ReplayError, ReplayOutcome, def_existence_unknown, replay,
};
use rim_merge::tree::{Content, FieldNode};
use rim_merge::xml;

/// `value[key]` as a string, or a panic naming which key was missing —
/// these helpers are not `#[test]` functions, so the workspace's
/// `clippy::expect_used` denial applies to them (`apps/cli`'s own
/// `assign_cli.rs` documents the same rule).
fn text(value: &serde_json::Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("the vendored file's {key:?} must be a string: {value}"))
        .to_string()
}

/// The bundled snapshot's own map, parsed from the very `rules/` file
/// `rim-io`'s `build.rs` embeds — **read, never restated**: a second
/// copy of these rows here would let the two drift and still pass,
/// which is the one failure mode splitting the table into a data file
/// introduces.
fn vendored_behaviours() -> PatchOperationBehaviours {
    let raw = include_str!("../../../rules/rimmerge-rules.json");
    let bundle: serde_json::Value = serde_json::from_str(raw)
        .unwrap_or_else(|error| panic!("the bundled rules file must parse: {error}"));
    let parsed = &bundle["patch_operations"];

    let mut classes = BTreeMap::new();
    for row in array_at(parsed, "classes") {
        let match_kind = ClassMatch::from_wire(&text(row, "match"))
            .unwrap_or_else(|| panic!("a match mode this binary implements: {row}"));
        let behaviour = CustomBehaviour::from_wire(&text(row, "behaviour"))
            .unwrap_or_else(|| panic!("a behaviour this binary implements: {row}"));
        classes.insert(
            text(row, "class"),
            ClassBehaviour {
                match_kind,
                behaviour,
            },
        );
    }

    let mut gates = Vec::new();
    for row in array_at(parsed, "gates") {
        let fields = &row["fields"];
        let mut conditional_types = BTreeMap::new();
        let declared = row["conditional_types"]
            .as_object()
            .unwrap_or_else(|| panic!("conditional_types must be an object: {row}"));
        for (name, kind) in declared {
            let kind = kind
                .as_str()
                .unwrap_or_else(|| panic!("a conditional kind must be a string: {row}"));
            conditional_types.insert(
                name.clone(),
                ConditionalKind::from_wire(kind)
                    .unwrap_or_else(|| panic!("a conditional kind this binary implements: {kind}")),
            );
        }
        gates.push(ClassGate {
            class: text(row, "class"),
            match_kind: ClassMatch::from_wire(&text(row, "match"))
                .unwrap_or_else(|| panic!("a match mode this binary implements: {row}")),
            behaviour: GateBehaviour::from_wire(&text(row, "behaviour"))
                .unwrap_or_else(|| panic!("a gate behaviour this binary implements: {row}")),
            fields: GateFields {
                requires_all: text(fields, "requires_all"),
                conditional_type: text(fields, "conditional_type"),
                conditional_param: text(fields, "conditional_param"),
            },
            conditional_types,
        });
    }

    PatchOperationBehaviours::new(classes, gates)
}

fn array_at<'a>(value: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("the vendored file's {key:?} must be an array"))
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/xml/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn replay_with(
    tree_xml: &str,
    operation_xml: &str,
    def_type: &str,
    def_name: &str,
    behaviours: &PatchOperationBehaviours,
) -> ReplayOutcome {
    let tree = xml::parse(tree_xml).unwrap_or_else(|error| panic!("well-formed fixture: {error}"));
    let active: BTreeSet<ModId> = [ModId::new("mod.a"), ModId::new("mod.b")]
        .into_iter()
        .collect();
    let names: BTreeMap<String, ModId> = BTreeMap::new();
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type,
        def_name,
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours,
    };
    replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_id,
            operation_xml,
        }],
        &context,
    )
}

fn children_of<'a>(outcome: &'a ReplayOutcome, path: &str) -> &'a [FieldNode] {
    let node = outcome
        .tree
        .get(
            &path
                .parse()
                .unwrap_or_else(|_| panic!("valid path: {path}")),
        )
        .unwrap_or_else(|| panic!("expected {path} to exist"));
    match &node.content {
        Content::Children(children) => children,
        other => panic!("expected children at {path}, got {other:?}"),
    }
}

fn text_at(outcome: &ReplayOutcome, path: &str) -> Content {
    outcome
        .tree
        .get(
            &path
                .parse()
                .unwrap_or_else(|_| panic!("valid path: {path}")),
        )
        .unwrap_or_else(|| panic!("expected {path} to exist"))
        .content
        .clone()
}

fn assert_unsupported(outcome: &ReplayOutcome, what: &str) {
    assert!(
        matches!(outcome.error, Some(ReplayError::Unsupported { .. })),
        "{what} must degrade to Unsupported with no map loaded, got {:?}",
        outcome.error
    );
}

// -- 1. set_mod_extension -------------------------------------------------

const SET_MOD_EXTENSION_DEF: &str = "ThingDef";
const SET_MOD_EXTENSION_NAME: &str = "EF_ChargedRifle";

#[test]
fn set_mod_extension_with_the_vendored_map_reproduces_the_shipped_behaviour() {
    let operation = fixture("safepatcher_set_mod_extension.xml");
    let behaviours = vendored_behaviours();

    let appended = replay_with(
        r#"<ThingDef><modExtensions><li Class="Other.Props"><v>keep</v></li></modExtensions></ThingDef>"#,
        &operation,
        SET_MOD_EXTENSION_DEF,
        SET_MOD_EXTENSION_NAME,
        &behaviours,
    );
    assert!(appended.error.is_none(), "{:?}", appended.error);
    let items = children_of(&appended, "modExtensions");
    assert_eq!(
        items.len(),
        2,
        "a different Class is appended, not replaced"
    );
    assert_eq!(
        items[1].attrs.get("Class").map(String::as_str),
        Some("ExampleFlash.FlashProps")
    );

    let replaced = replay_with(
        r#"<ThingDef><modExtensions><li Class="ExampleFlash.FlashProps"><def>OLD</def></li></modExtensions></ThingDef>"#,
        &operation,
        SET_MOD_EXTENSION_DEF,
        SET_MOD_EXTENSION_NAME,
        &behaviours,
    );
    let items = children_of(&replaced, "modExtensions");
    assert_eq!(items.len(), 1, "the same Class is replaced in place");
    let Content::Children(fields) = &items[0].content else {
        panic!("the replacing item has children");
    };
    assert_eq!(
        fields[0].content,
        Content::Text("EF_ChargedRifleFlash".to_string())
    );

    let created = replay_with(
        "<ThingDef/>",
        &operation,
        SET_MOD_EXTENSION_DEF,
        SET_MOD_EXTENSION_NAME,
        &behaviours,
    );
    assert_eq!(
        children_of(&created, "modExtensions").len(),
        1,
        "the container is created when absent"
    );
}

#[test]
fn set_mod_extension_with_an_empty_map_is_unsupported() {
    let outcome = replay_with(
        r#"<ThingDef><modExtensions/></ThingDef>"#,
        &fixture("safepatcher_set_mod_extension.xml"),
        SET_MOD_EXTENSION_DEF,
        SET_MOD_EXTENSION_NAME,
        PatchOperationBehaviours::none(),
    );
    assert_unsupported(&outcome, "SafePatcher.PatchOperationSetModExtension");
}

// -- 2. add_or_replace ----------------------------------------------------

const ADD_OR_REPLACE_TREE: &str =
    "<ResearchProjectDef><prerequisites><li>OldPrereq</li></prerequisites></ResearchProjectDef>";
const ADD_OR_REPLACE_DEF: &str = "ResearchProjectDef";
const ADD_OR_REPLACE_NAME: &str = "FertilityProcedures";

#[test]
fn add_or_replace_with_the_vendored_map_reproduces_the_shipped_behaviour() {
    let outcome = replay_with(
        ADD_OR_REPLACE_TREE,
        &fixture("rr_add_or_replace.xml"),
        ADD_OR_REPLACE_DEF,
        ADD_OR_REPLACE_NAME,
        &vendored_behaviours(),
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let prerequisites = children_of(&outcome, "prerequisites");
    assert_eq!(prerequisites.len(), 1, "the existing field is replaced");
    assert_eq!(
        prerequisites[0].content,
        Content::Text("SterileMaterials".to_string())
    );
    assert_eq!(
        text_at(&outcome, "techLevel"),
        Content::Text("Spacer".to_string()),
        "the missing field is added"
    );
}

#[test]
fn add_or_replace_with_an_empty_map_is_unsupported() {
    let outcome = replay_with(
        ADD_OR_REPLACE_TREE,
        &fixture("rr_add_or_replace.xml"),
        ADD_OR_REPLACE_DEF,
        ADD_OR_REPLACE_NAME,
        PatchOperationBehaviours::none(),
    );
    assert_unsupported(&outcome, "RR.PatchOperationAddOrReplace");
}

// -- 3. replace_research_coords, and its gate -----------------------------

const COORDS_TREE: &str = r#"<ResearchProjectDef><researchViewX>1.0</researchViewX><researchViewY>2.0</researchViewY><label>Old</label></ResearchProjectDef>"#;
const COORDS_DEF: &str = "ResearchProjectDef";
const COORDS_NAME: &str = "SteppingStone1";

fn coords_operation(extra: &str) -> String {
    format!(
        r#"<Operation Class="RR.PatchOperationReplaceResearchCoords">
             <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
             {extra}
             <researchViewX>5.5</researchViewX>
             <researchViewY>6.5</researchViewY>
           </Operation>"#
    )
}

#[test]
fn replace_research_coords_with_the_vendored_map_reproduces_the_shipped_behaviour() {
    let outcome = replay_with(
        COORDS_TREE,
        &coords_operation(""),
        COORDS_DEF,
        COORDS_NAME,
        &vendored_behaviours(),
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "researchViewX"),
        Content::Text("5.5".to_string())
    );
    assert_eq!(
        text_at(&outcome, "researchViewY"),
        Content::Text("6.5".to_string())
    );
    assert_eq!(
        text_at(&outcome, "label"),
        Content::Text("Old".to_string()),
        "every other field is left untouched"
    );
}

/// The `RR.` prefix gate, through the vendored data: `doesRequire` naming
/// an inactive mod skips the operation outright (a no-op, not a failure),
/// and each declared `conditionalType` resolves the way the IL does.
#[test]
fn the_vendored_gate_reproduces_the_shipped_skip_behaviour() {
    let behaviours = vendored_behaviours();

    let required_missing = replay_with(
        COORDS_TREE,
        &coords_operation("<doesRequire>mod.c</doesRequire>"),
        COORDS_DEF,
        COORDS_NAME,
        &behaviours,
    );
    assert!(required_missing.error.is_none());
    assert_eq!(
        text_at(&required_missing, "researchViewX"),
        Content::Text("1.0".to_string()),
        "an unmet doesRequire skips the operation"
    );

    let cond_loaded_missing = replay_with(
        COORDS_TREE,
        &coords_operation(
            "<conditionalType>Cond_ModsLoaded</conditionalType><conditionalParam>mod.c</conditionalParam>",
        ),
        COORDS_DEF,
        COORDS_NAME,
        &behaviours,
    );
    assert_eq!(
        text_at(&cond_loaded_missing, "researchViewX"),
        Content::Text("1.0".to_string()),
        "Cond_ModsLoaded skips when the named mod is inactive"
    );

    let cond_loaded_present = replay_with(
        COORDS_TREE,
        &coords_operation(
            "<conditionalType>Cond_ModsLoaded</conditionalType><conditionalParam>mod.a</conditionalParam>",
        ),
        COORDS_DEF,
        COORDS_NAME,
        &behaviours,
    );
    assert_eq!(
        text_at(&cond_loaded_present, "researchViewX"),
        Content::Text("5.5".to_string()),
        "and runs when it is active"
    );

    let cond_not_loaded = replay_with(
        COORDS_TREE,
        &coords_operation(
            "<conditionalType>Cond_ModsNotLoaded</conditionalType><conditionalParam>mod.a</conditionalParam>",
        ),
        COORDS_DEF,
        COORDS_NAME,
        &behaviours,
    );
    assert_eq!(
        text_at(&cond_not_loaded, "researchViewX"),
        Content::Text("1.0".to_string()),
        "Cond_ModsNotLoaded is the mirror"
    );

    let unmodelled = replay_with(
        COORDS_TREE,
        &coords_operation("<conditionalType>Cond_Surgery</conditionalType>"),
        COORDS_DEF,
        COORDS_NAME,
        &behaviours,
    );
    assert_unsupported(
        &unmodelled,
        "a conditionalType the gate's own data does not declare",
    );
}

#[test]
fn replace_research_coords_with_an_empty_map_is_unsupported() {
    let outcome = replay_with(
        COORDS_TREE,
        &coords_operation(""),
        COORDS_DEF,
        COORDS_NAME,
        PatchOperationBehaviours::none(),
    );
    assert_unsupported(&outcome, "RR.PatchOperationReplaceResearchCoords");
}

// -- 4. every other fixture is unaffected by the map ----------------------

/// Fixtures this test does not replay, each for a stated reason — the
/// **only** way a file under `tests/fixtures/xml/` may go uncovered.
/// Every other file there is discovered by [`std::fs::read_dir`] and must
/// appear in [`REPLAY_CASES`], so adding a fixture and forgetting this
/// test fails loudly instead of silently shrinking the comparison.
const NOT_REPLAYED: &[(&str, &str)] = &[
    (
        "safepatcher_set_mod_extension.xml",
        "a mapped class — covered by its own with-map/without-map pair above",
    ),
    (
        "rr_add_or_replace.xml",
        "a mapped class — covered by its own with-map/without-map pair above",
    ),
    (
        "core_bionic_heart.xml",
        "a def document, not an <Operation> — closure.rs's own input",
    ),
    (
        "core_hediff_bases.xml",
        "a template document, not an <Operation>",
    ),
    (
        "core_temperate_forest.xml",
        "a def document, not an <Operation>",
    ),
    (
        "core_arid_shrubland_wild_animals.xml",
        "a def document, not an <Operation>",
    ),
    (
        "bionics_bionic_heart.xml",
        "a def document, not an <Operation>",
    ),
    (
        "bionics_hediff_base.xml",
        "a template document, not an <Operation>",
    ),
    (
        "arid_shrubland_wild_animals_patches.xml",
        "a multi-operation <Patch> file, not one standalone <Operation>",
    ),
];

/// The structural detections stay in code and stay name-free, so every
/// fixture that does *not* use one of the three mapped classes must
/// replay identically with the vendored map and with an empty one. This
/// is the other half of "the vendored default reproduces today's
/// behaviour": the map must not change anything it was not asked to.
///
/// `(fixture, tree, def type, def name)`. Kept beside [`NOT_REPLAYED`]
/// so the two together account for every file in the fixture directory —
/// see [`every_xml_fixture_is_either_replayed_here_or_explicitly_skipped`].
const REPLAY_CASES: &[(&str, &str, &str, &str)] = &[
    (
        "sandbags_tex_path_text_replace.xml",
        "<ThingDef><graphicData><texPath>old</texPath></graphicData></ThingDef>",
        "ThingDef",
        "Sandbags",
    ),
    (
        "furniture_mod_option.xml",
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        "ThingDef",
        "Table2x2c",
    ),
    (
        "flora_plant_density.xml",
        "<BiomeDef><plantDensity>0.5</plantDensity></BiomeDef>",
        "BiomeDef",
        "AridShrubland",
    ),
    (
        "flora_plant_density_multi_biome.xml",
        "<BiomeDef><plantDensity>0.5</plantDensity></BiomeDef>",
        "BiomeDef",
        "AridShrubland",
    ),
    (
        "biomes_plant_density.xml",
        "<BiomeDef><plantDensity>0.5</plantDensity></BiomeDef>",
        "BiomeDef",
        "AridShrubland",
    ),
    (
        "armoury_quest_reward.xml",
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        "ThingDef",
        "Gun_Autopistol",
    ),
    (
        "remedies_insert_success_always.xml",
        "<ThingDef><comps><li><x>1</x></li></comps></ThingDef>",
        "ThingDef",
        "Penoxycyline",
    ),
    (
        "fishing_cross_def_conditional.xml",
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        "ThingDef",
        "EXF_FishingSpot",
    ),
];

/// Every `.xml` under `tests/fixtures/xml/` is either replayed by
/// [`every_other_fixture_replays_identically_with_and_without_the_map`]
/// or listed in [`NOT_REPLAYED`] with a reason. A new fixture that is
/// neither fails here — the hand-maintained list can no longer drift
/// out of date while claiming to be complete.
#[test]
fn every_xml_fixture_is_either_replayed_here_or_explicitly_skipped() {
    let dir = format!("{}/tests/fixtures/xml", env!("CARGO_MANIFEST_DIR"));
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|error| panic!("read {dir}: {error}"));

    let mut unaccounted = Vec::new();
    let mut seen = 0usize;
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("read {dir}: {error}"));
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".xml") {
            continue;
        }
        seen += 1;
        let replayed = REPLAY_CASES.iter().any(|(fixture, ..)| *fixture == name);
        let skipped = NOT_REPLAYED.iter().any(|(fixture, _)| *fixture == name);
        if !replayed && !skipped {
            unaccounted.push(name);
        }
    }

    assert!(
        unaccounted.is_empty(),
        "these fixtures are neither replayed nor listed in NOT_REPLAYED with a reason: \
         {unaccounted:?} — add each to one list or the other"
    );
    // The directory must not have become empty (a moved fixture folder
    // would otherwise make this whole file vacuously pass).
    assert!(
        seen >= REPLAY_CASES.len() + NOT_REPLAYED.len(),
        "expected at least {} fixtures on disk, found {seen}",
        REPLAY_CASES.len() + NOT_REPLAYED.len()
    );
}

#[test]
fn every_other_fixture_replays_identically_with_and_without_the_map() {
    let behaviours = vendored_behaviours();
    let mut replayed_cleanly = 0usize;
    for (name, tree, def_type, def_name) in REPLAY_CASES {
        let operation = fixture(name);
        let with_map = replay_with(tree, &operation, def_type, def_name, &behaviours);
        let without_map = replay_with(
            tree,
            &operation,
            def_type,
            def_name,
            PatchOperationBehaviours::none(),
        );

        assert_eq!(
            with_map.tree, without_map.tree,
            "{name}: the class map must not change a fixture that uses none of its classes"
        );
        assert_eq!(with_map.error, without_map.error, "{name}: same outcome");
        assert_eq!(
            with_map.caveats, without_map.caveats,
            "{name}: same caveats"
        );
        if with_map.error.is_none() {
            replayed_cleanly += 1;
        }
    }

    // The anti-vacuous-green guard: "identical with and without the map"
    // is trivially true if every case merely fails identically. Several
    // must actually replay.
    assert!(
        replayed_cleanly >= 4,
        "only {replayed_cleanly} of {} fixtures replayed without an error — this test would \
         otherwise be comparing two identical failures",
        REPLAY_CASES.len()
    );
}
