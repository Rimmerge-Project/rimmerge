//! Container (list) scenarios: list items with and without identity, list position, and replace/add
//! on a container.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{DefEntry, ModId, PatchOp, Report, Selector, XmlLocator};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`two_mod_list_fixture`] builds.
pub struct TwoModListFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// both patchers' top-level operations (a `label` replace each, plus
    /// a `comps` add each).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `b.mod` both patch `ThingDef/Widget`'s `label` to
/// different values (the collision) *and* each add a distinct `li` item
/// to its `comps` list — the two-mod list fixture: the final list holds
/// both items, each its own
/// `FieldRowKind::ListEntry` row naming its own mod.
#[must_use]
pub fn two_mod_list_fixture() -> TwoModListFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget.xml", 0);
    let a_label_locator = locator("a_patch.xml", 0);
    let a_comps_locator = locator("a_patch.xml", 1);
    let b_label_locator = locator("b_patch.xml", 0);
    let b_comps_locator = locator("b_patch.xml", 1);

    let mut sources = SourceIndex::default();
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

    let make_replace = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
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
    let make_add = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
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
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![
            make_replace(
                &mod_a,
                r#"Defs/ThingDef[defName="Widget"]/label"#,
                a_label_locator.clone(),
            ),
            make_add(
                &mod_a,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                a_comps_locator.clone(),
            ),
            make_replace(
                &mod_b,
                r#"Defs/ThingDef[defName="Widget"]/label"#,
                b_label_locator.clone(),
            ),
            make_add(
                &mod_b,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                b_comps_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><label>widget</label><comps></comps></ThingDef>"
            .to_string(),
    );
    elements.insert(
        a_label_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Widget"]/label</xpath>
             <value><label>alpha widget</label></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_comps_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_label_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Widget"]/label</xpath>
             <value><label>beta widget</label></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_comps_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Beta"/></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    TwoModListFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// [`two_mod_list_fixture`]'s own twin, but each mod's own added `li` has
/// no `Class` attribute, no well-known key child (`rim_merge::tree`'s own
/// `KEY_CHILDREN` list), and no text content — `<li><amount>5</amount></li>`/
/// `<li><amount>7</amount></li>` — so `ItemIdentity::identify` falls all
/// the way back to `ItemId::Position` for both. Pins that
/// `is_container_conflict_explained_by_list_children`'s "never
/// `ItemId::Position`" rule isn't only about two mods declaring the exact
/// same `Class` — an identity-less `li` hits the identical fallback, and
/// the container conflict must stay a genuine `Conflict` for the same
/// reason (a `Position` numbering from one candidate's own isolated
/// single-item subtree can never be safely compared against another's).
#[must_use]
pub fn identity_less_list_items_fixture() -> TwoModListFixture {
    let core = ModId::new("core.mod");
    let mod_p = ModId::new("p.mod");
    let mod_q = ModId::new("q.mod");

    let core_locator = locator("core_trinket.xml", 0);
    let p_label_locator = locator("p_patch.xml", 0);
    let p_comps_locator = locator("p_patch.xml", 1);
    let q_label_locator = locator("q_patch.xml", 0);
    let q_comps_locator = locator("q_patch.xml", 1);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("ThingDef".to_string(), "Trinket".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Trinket".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Trinket".to_string()),
        vec![core.clone()],
    );

    let make_replace = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
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
    let make_add = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
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
            "ThingDef".to_string(),
            "Trinket".to_string(),
            Selector::DefName,
        ),
        vec![
            make_replace(
                &mod_p,
                r#"Defs/ThingDef[defName="Trinket"]/label"#,
                p_label_locator.clone(),
            ),
            make_add(
                &mod_p,
                r#"Defs/ThingDef[defName="Trinket"]/comps"#,
                p_comps_locator.clone(),
            ),
            make_replace(
                &mod_q,
                r#"Defs/ThingDef[defName="Trinket"]/label"#,
                q_label_locator.clone(),
            ),
            make_add(
                &mod_q,
                r#"Defs/ThingDef[defName="Trinket"]/comps"#,
                q_comps_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Trinket</defName><label>trinket</label><comps></comps></ThingDef>"
            .to_string(),
    );
    elements.insert(
        p_label_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Trinket"]/label</xpath>
             <value><label>p trinket</label></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        p_comps_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Trinket"]/comps</xpath>
             <value><li><amount>5</amount></li></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        q_label_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Trinket"]/label</xpath>
             <value><label>q trinket</label></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        q_comps_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Trinket"]/comps</xpath>
             <value><li><amount>7</amount></li></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("p.mod")
        .mod_("q.mod")
        .patch_collision("ThingDef", "Trinket", &["p.mod", "q.mod"])
        .build();

    TwoModListFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`three_mod_list_fixture`] builds.
pub struct ThreeModListFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, `b.mod`, and `c.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Gadget` and
    /// three patchers' own `comps`-adding operations, one distinct `li`
    /// item each.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// Three mods each add a distinct `li` item to `ThingDef/Gadget`'s
/// `comps` — the three-mod list fixture: the final list holds all three,
/// each its own
/// `FieldRowKind::ListEntry` row naming its own mod.
#[must_use]
pub fn three_mod_list_fixture() -> ThreeModListFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let mod_c = ModId::new("c.mod");

    let core_locator = locator("core_gadget.xml", 0);
    let a_locator = locator("a3_patch.xml", 0);
    let b_locator = locator("b3_patch.xml", 0);
    let c_locator = locator("c3_patch.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (core.clone(), ("ThingDef".to_string(), "Gadget".to_string())),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Gadget".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Gadget".to_string()),
        vec![core.clone()],
    );

    let make_add = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
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
            "ThingDef".to_string(),
            "Gadget".to_string(),
            Selector::DefName,
        ),
        vec![
            make_add(
                &mod_a,
                r#"Defs/ThingDef[defName="Gadget"]/comps"#,
                a_locator.clone(),
            ),
            make_add(
                &mod_b,
                r#"Defs/ThingDef[defName="Gadget"]/comps"#,
                b_locator.clone(),
            ),
            make_add(
                &mod_c,
                r#"Defs/ThingDef[defName="Gadget"]/comps"#,
                c_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Gadget</defName><comps></comps></ThingDef>".to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Gadget"]/comps</xpath>
             <value><li Class="CompProperties_A"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Gadget"]/comps</xpath>
             <value><li Class="CompProperties_B"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Gadget"]/comps</xpath>
             <value><li Class="CompProperties_C"/></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("ThingDef", "Gadget", &["a.mod", "b.mod", "c.mod"])
        .build();

    ThreeModListFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`list_position_differs_from_lexical_order_fixture`] builds.
pub struct ListPositionFixture {
    /// A [`Report`] naming `core.mod`, `z.mod`, and `a.mod` (in that load
    /// order — `z.mod` loads *first*, despite its `ModId` sorting after
    /// `a.mod`'s).
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Sprocket` and
    /// both patchers' `comps`-adding operations.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `z.mod` (loading *first*) adds `li Class="Zeta"` to `ThingDef/Sprocket`'s
/// `comps`; `a.mod` (loading *second*) adds `li Class="Alpha"`. Document
/// position has Zeta first, but `FieldPath`'s own lexical `Ord` would put
/// Alpha first (`"Alpha" < "Zeta"`) — this
/// proves `sort_field_rows` orders `ListEntry` siblings by their actual
/// position in `effective.resolved`, not by path.
#[must_use]
pub fn list_position_differs_from_lexical_order_fixture() -> ListPositionFixture {
    let core = ModId::new("core.mod");
    let mod_z = ModId::new("z.mod");
    let mod_a = ModId::new("a.mod");

    let core_locator = locator("core_sprocket.xml", 0);
    let z_locator = locator("z_patch.xml", 0);
    let a_locator = locator("a6_patch.xml", 0);

    let mut sources = SourceIndex::default();
    sources.defs.insert(
        (
            core.clone(),
            ("ThingDef".to_string(), "Sprocket".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Sprocket".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Sprocket".to_string()),
        vec![core.clone()],
    );

    let make_add = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
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
            "ThingDef".to_string(),
            "Sprocket".to_string(),
            Selector::DefName,
        ),
        vec![
            make_add(
                &mod_z,
                r#"Defs/ThingDef[defName="Sprocket"]/comps"#,
                z_locator.clone(),
            ),
            make_add(
                &mod_a,
                r#"Defs/ThingDef[defName="Sprocket"]/comps"#,
                a_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Sprocket</defName><comps></comps></ThingDef>".to_string(),
    );
    elements.insert(
        z_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Sprocket"]/comps</xpath>
             <value><li Class="Zeta"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Sprocket"]/comps</xpath>
             <value><li Class="Alpha"/></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("z.mod")
        .mod_("a.mod")
        .patch_collision("ThingDef", "Sprocket", &["z.mod", "a.mod"])
        .build();

    ListPositionFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`replace_vs_replace_on_container_fixture`] builds.
pub struct ReplaceVsReplaceFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// both patchers' own wholesale `PatchOperationReplace` of `comps`.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `b.mod` each `PatchOperationReplace` the *whole* `comps`
/// container with their own single, distinct-`Class` `li` — `b.mod` loads
/// last, so its own `Replace` wholesale discards `a.mod`'s own content in
/// the real, full fold. `a.mod`'s own candidate item never survives in
/// `effective.resolved`, so the container-level `Conflict` row must stay
/// — a genuine conflict (data was actually lost), never demoted to a
/// clean list merge.
#[must_use]
pub fn replace_vs_replace_on_container_fixture() -> ReplaceVsReplaceFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget3.xml", 0);
    let a_locator = locator("a7_patch.xml", 0);
    let b_locator = locator("b7_patch.xml", 0);

    let mut sources = SourceIndex::default();
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

    let make_replace = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
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
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![
            make_replace(
                &mod_a,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                a_locator.clone(),
            ),
            make_replace(
                &mod_b,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                b_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
        r#"<ThingDef><defName>Widget</defName><comps><li Class="CompProperties_Original"/></comps></ThingDef>"#
            .to_string());
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><comps><li Class="CompProperties_Alpha"/></comps></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><comps><li Class="CompProperties_Beta"/></comps></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    ReplaceVsReplaceFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`add_then_replace_on_container_fixture`] builds.
pub struct AddThenReplaceFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget`,
    /// `a.mod`'s `PatchOperationAdd` on `comps`, and `b.mod`'s wholesale
    /// `PatchOperationReplace` of it.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` adds its own `li` to `comps`; `b.mod`, loading after, replaces
/// the *whole* `comps` container wholesale — `a.mod`'s own item never
/// survives the real fold either, the same shape (and same required
/// outcome) as [`replace_vs_replace_on_container_fixture`], but proving
/// the survival check catches a clobber regardless of which op class did
/// the clobbering.
#[must_use]
pub fn add_then_replace_on_container_fixture() -> AddThenReplaceFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget4.xml", 0);
    let a_locator = locator("a8_patch.xml", 0);
    let b_locator = locator("b8_patch.xml", 0);

    let mut sources = SourceIndex::default();
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
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_a,
                "PatchOperationAdd",
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                a_locator.clone(),
            ),
            make_op(
                &mod_b,
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                b_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><comps><li Class="CompProperties_Beta"/></comps></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    AddThenReplaceFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`list_with_owner_item_fixture`] builds.
pub struct ListWithOwnerItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget`
    /// (whose raw `comps` already has one `li`) and both patchers' own
    /// distinct `PatchOperationAdd`s onto it.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `core.mod`'s own raw `comps` already has one, pre-existing
/// (`Owner`-credited) `li`; `a.mod` and `b.mod` each add their own,
/// further distinct-`Class` `li`. A real, non-empty base list
/// must still let the container-level
/// `Conflict` row demote away — every item (the base's own included)
/// survives `effective.resolved` unchanged, and both patchers' own items
/// are `Patch`-credited, so this is exactly as "explained" as the
/// empty-base case `two_mod_list_fixture` already covers.
#[must_use]
pub fn list_with_owner_item_fixture() -> ListWithOwnerItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget5.xml", 0);
    let a_locator = locator("a9_patch.xml", 0);
    let b_locator = locator("b9_patch.xml", 0);

    let mut sources = SourceIndex::default();
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

    let make_add = |mod_id: &ModId, xpath: &str, op_locator: XmlLocator| IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
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
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![
            make_add(
                &mod_a,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                a_locator.clone(),
            ),
            make_add(
                &mod_b,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                b_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(core_locator,
        r#"<ThingDef><defName>Widget</defName><comps><li Class="CompProperties_Base"/></comps></ThingDef>"#
            .to_string());
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"/></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Beta"/></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    ListWithOwnerItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
