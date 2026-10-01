//! Biome wild-animal list scenarios (map-keyed list items).

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{DefEntry, ModId, PatchOp, Report, Selector, XmlLocator};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`arid_shrubland_wild_animals_fixture`] builds.
pub struct AridShrublandWildAnimalsFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, `b.mod`, `c.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `BiomeDef/AridShrubland`
    /// and each of the three patchers' own two top-level operations on
    /// `wildAnimals` (one disjoint `PatchOperationAdd`, one
    /// `PatchOperationReplace` of the pre-existing `Cobra` key).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// A real-install keyed-map shape, built so it reaches
/// [`rim_merge::diff::collision_fields`]'s per-key expansion through the
/// *real* replay pipeline (`PlanMerge`) rather than a hand-built
/// [`rim_merge::diff::FieldDiff`] — deliberately, not `rim-merge`'s own
/// fixture: `BiomeDef/AridShrubland`'s `wildAnimals`, a
/// `Dictionary<PawnKindDef, float>`, starts with five keys (`Tortoise`/
/// `Cobra`/`Warg`/`Camel`/`Locust`); `a.mod`/`b.mod`/`c.mod` each add
/// their own disjoint animal (`Allosaurus`/`Mammoth`/`Hyena` —
/// `OneSided`, cleanly auto-resolving, the same shape `rim-merge`'s own
/// `plan.rs` integration test already proves) **and** each independently
/// `PatchOperationReplace`s the pre-existing `Cobra` key: `a.mod`/`b.mod`
/// to the same `0.5` (`Agreeing`), `c.mod` to a differing `0.9`
/// (promotes the whole key to `Conflict{a,b,c}`).
///
/// **Deliberately not a literal `Add`/`Add`/`Add` collision on one new
/// key**: a real same-tag `Add`/`Add` collision produces duplicate
/// `<Raptor>` siblings once replayed (`container_kind` correctly
/// classifies that `Record`, never `KeyedMap`), so it never reaches
/// `collision_fields`'s expand branch through `patch_eval::replay` at all.
/// `Cobra` is a `Replace` of a key already present in `target_raw`
/// instead — each contributor's own isolated replay succeeds independently
/// with no dependency on another mod's earlier op, so it reaches the real
/// `Conflict`/`Agreeing` classification safely. The "mod A `Add`s a key,
/// mods B/C `Replace` it" shape, whose isolated replays need correcting,
/// is [`add_then_replace_map_key_fixture`]'s job instead.
#[must_use]
pub fn arid_shrubland_wild_animals_fixture() -> AridShrublandWildAnimalsFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let mod_c = ModId::new("c.mod");

    let core_locator = locator("core_arid_shrubland.xml", 0);
    let a_add_locator = locator("a_wild_animals_patch.xml", 0);
    let a_replace_locator = locator("a_wild_animals_patch.xml", 1);
    let b_add_locator = locator("b_wild_animals_patch.xml", 0);
    let b_replace_locator = locator("b_wild_animals_patch.xml", 1);
    let c_add_locator = locator("c_wild_animals_patch.xml", 0);
    let c_replace_locator = locator("c_wild_animals_patch.xml", 1);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("BiomeDef".to_string(), "AridShrubland".to_string()),
        ),
        vec![DefEntry {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("BiomeDef".to_string(), "AridShrubland".to_string()),
        vec![core.clone()],
    );

    let make_op =
        |mod_id: &ModId, class: &str, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
            mod_id: mod_id.clone(),
            op: PatchOp {
                injected_template_names: std::collections::BTreeSet::new(),
                class: class.to_string(),
                xpath: Some(xpath.to_string()),
                target: None,
                find_mod_context: Vec::new(),
                find_mod_names: Vec::new(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                is_mutating: true,
                injected_types: BTreeSet::new(),
                injected_paths: BTreeSet::new(),
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
                locator: op_locator,
            },
        };
    sources.patch_ops_by_def.insert(
        (
            "BiomeDef".to_string(),
            "AridShrubland".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_a,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                a_add_locator.clone(),
            ),
            make_op(
                &mod_a,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra"#,
                a_replace_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                b_add_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra"#,
                b_replace_locator.clone(),
            ),
            make_op(
                &mod_c,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                c_add_locator.clone(),
            ),
            make_op(
                &mod_c,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra"#,
                c_replace_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<BiomeDef><defName>AridShrubland</defName><wildAnimals>\
         <Tortoise>0.4</Tortoise><Cobra>0.3</Cobra><Warg>0.2</Warg>\
         <Camel>0.1</Camel><Locust>0.05</Locust></wildAnimals></BiomeDef>"
            .to_string(),
    );
    elements.insert(
        a_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Allosaurus>0.6</Allosaurus></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra</xpath>
             <value><Cobra>0.5</Cobra></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Mammoth>0.1</Mammoth></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra</xpath>
             <value><Cobra>0.5</Cobra></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Hyena>0.2</Hyena></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra</xpath>
             <value><Cobra>0.9</Cobra></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("BiomeDef", "AridShrubland", &["a.mod", "b.mod", "c.mod"])
        .build();

    AridShrublandWildAnimalsFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`arid_shrubland_wild_animals_fixture`]'s own two-mod sibling: only
/// `a.mod`/`b.mod` (no `c.mod`), both replacing `Cobra` to the *same*
/// `0.5` — a `DiffClass::Agreeing` map entry, never a `Conflict`, so
/// `def_conflict_view`'s own "also added by" fold
/// (`map_entry_values_and_agreed_by`) has something to fold: `a.mod`
/// (the earliest in load order) keeps the row's own `values`, `b.mod`
/// lands in `agreed_by`. A genuinely separate, self-contained fixture
/// rather than the three-mod one's `mods` field narrowed to `{a, b}` —
/// narrowing would leave `c.mod`'s own real `Cobra` replace (`0.9`)
/// still active in the underlying install, so `InspectDef`'s own
/// `in_game` column would read `0.9` while this collision's own `values`
/// read `0.5`, a confusing (if not incorrect) mismatch this fixture
/// avoids by never giving `c.mod` a `Cobra` op at all.
#[must_use]
pub fn arid_shrubland_wild_animals_two_mod_fixture() -> AridShrublandWildAnimalsFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_arid_shrubland2.xml", 0);
    let a_add_locator = locator("a2_wild_animals_patch.xml", 0);
    let a_replace_locator = locator("a2_wild_animals_patch.xml", 1);
    let b_add_locator = locator("b2_wild_animals_patch.xml", 0);
    let b_replace_locator = locator("b2_wild_animals_patch.xml", 1);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("BiomeDef".to_string(), "AridShrubland".to_string()),
        ),
        vec![DefEntry {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("BiomeDef".to_string(), "AridShrubland".to_string()),
        vec![core.clone()],
    );

    let make_op =
        |mod_id: &ModId, class: &str, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
            mod_id: mod_id.clone(),
            op: PatchOp {
                injected_template_names: std::collections::BTreeSet::new(),
                class: class.to_string(),
                xpath: Some(xpath.to_string()),
                target: None,
                find_mod_context: Vec::new(),
                find_mod_names: Vec::new(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                is_mutating: true,
                injected_types: BTreeSet::new(),
                injected_paths: BTreeSet::new(),
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
                locator: op_locator,
            },
        };
    sources.patch_ops_by_def.insert(
        (
            "BiomeDef".to_string(),
            "AridShrubland".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_a,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                a_add_locator.clone(),
            ),
            make_op(
                &mod_a,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra"#,
                a_replace_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                b_add_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra"#,
                b_replace_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<BiomeDef><defName>AridShrubland</defName><wildAnimals>\
         <Tortoise>0.4</Tortoise><Cobra>0.3</Cobra><Warg>0.2</Warg>\
         <Camel>0.1</Camel><Locust>0.05</Locust></wildAnimals></BiomeDef>"
            .to_string(),
    );
    elements.insert(
        a_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Allosaurus>0.6</Allosaurus></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra</xpath>
             <value><Cobra>0.5</Cobra></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Mammoth>0.1</Mammoth></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Cobra</xpath>
             <value><Cobra>0.5</Cobra></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("BiomeDef", "AridShrubland", &["a.mod", "b.mod"])
        .build();

    AridShrublandWildAnimalsFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`arid_shrubland_wild_animals_fixture`]'s own add-then-replace sibling:
/// `a.mod` `PatchOperationAdd`s a *fresh* `Raptor` key into
/// Core's own `wildAnimals` map, then `b.mod` and `c.mod` each
/// `PatchOperationReplace` that same key — `b.mod` to `a.mod`'s own
/// `0.3`, `c.mod` (loading last, so it wins in game) to `0.9`.
///
/// The shape exists to pin the *display* path against the *plan* path.
/// Neither `b.mod`'s nor `c.mod`'s own `Replace` matches anything when
/// replayed in isolation — the key doesn't exist without `a.mod`'s own
/// `Add` — so an isolated per-mod candidate set reads them both as
/// `Value::Absent` and misclassifies the key as a clean
/// `DiffClass::OneSided { by: a.mod }`. `rim_merge::plan::plan_patch_collision`
/// corrects that (a member whose isolated replay produced a
/// `Caveat::FailedOp` naming it is rebuilt via `move_mod_last`), so the
/// key is really a three-way `DiffClass::Conflict` that needs the user's
/// own input. `rim-session` consumes that candidate construction rather
/// than mirroring it, so the def-conflict view must never show a clean
/// `MapEntry` row for a key the plan put in `unresolved`.
#[must_use]
pub fn add_then_replace_map_key_fixture() -> AridShrublandWildAnimalsFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let mod_c = ModId::new("c.mod");

    let core_locator = locator("core_arid_shrubland_b1.xml", 0);
    let a_add_locator = locator("a_b1_raptor_patch.xml", 0);
    let b_replace_locator = locator("b_b1_raptor_patch.xml", 0);
    let c_replace_locator = locator("c_b1_raptor_patch.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("BiomeDef".to_string(), "AridShrubland".to_string()),
        ),
        vec![DefEntry {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("BiomeDef".to_string(), "AridShrubland".to_string()),
        vec![core.clone()],
    );

    let make_op =
        |mod_id: &ModId, class: &str, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
            mod_id: mod_id.clone(),
            op: PatchOp {
                injected_template_names: std::collections::BTreeSet::new(),
                class: class.to_string(),
                xpath: Some(xpath.to_string()),
                target: None,
                find_mod_context: Vec::new(),
                find_mod_names: Vec::new(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                is_mutating: true,
                injected_types: BTreeSet::new(),
                injected_paths: BTreeSet::new(),
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
                locator: op_locator,
            },
        };
    sources.patch_ops_by_def.insert(
        (
            "BiomeDef".to_string(),
            "AridShrubland".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_a,
                "PatchOperationAdd",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals"#,
                a_add_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Raptor"#,
                b_replace_locator.clone(),
            ),
            make_op(
                &mod_c,
                "PatchOperationReplace",
                r#"Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Raptor"#,
                c_replace_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<BiomeDef><defName>AridShrubland</defName><wildAnimals>\
         <Tortoise>0.4</Tortoise><Cobra>0.3</Cobra></wildAnimals></BiomeDef>"
            .to_string(),
    );
    elements.insert(
        a_add_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
             <value><Raptor>0.3</Raptor></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Raptor</xpath>
             <value><Raptor>0.3</Raptor></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_replace_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals/Raptor</xpath>
             <value><Raptor>0.9</Raptor></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("BiomeDef", "AridShrubland", &["a.mod", "b.mod", "c.mod"])
        .build();

    AridShrublandWildAnimalsFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
