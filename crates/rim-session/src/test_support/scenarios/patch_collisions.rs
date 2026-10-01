//! Patch-collision scenarios: a whole-def override colliding with a patch, unsupported ops, and a
//! stopper after a contested field.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{DefEntry, ModId, PatchOp, Report, Selector, XmlLocator};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`whole_def_patch_collision_fixture`] builds.
pub struct WholeDefPatchCollisionFixture {
    /// A [`Report`] naming `core.mod` and `a.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// `a.mod`'s single top-level operation, targeting the def node
    /// itself (no `sub_path`).
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// A `PatchCollision` whose op's xpath is the bare def predicate —
/// `Defs/ThingDef[defName="Widget"]`, no trailing segment — so the
/// analyzer's own collision key carries `sub_path: None` (and
/// `plan_merge.rs`'s `path_key` then
/// defaults to the empty `FieldPath` rather than one built from a real
/// `sub_path_text`). `a.mod`'s own `PatchOperationAdd` appends a new
/// `<techLevel>` child directly under the def root, so its replayed
/// candidate genuinely differs from `core.mod`'s own base value
/// (`DiffClass::OneSided`) — the shape `rows_from_diff`/`after_merge_tree`
/// need to reach `rim_merge::plan::build_resolved_node` with an
/// empty-path, `Value::Item` field once the collision plans `Complete`.
/// A single contributing mod is deliberately used (the analysis crate's
/// own `ReportBuilder::patch_collision` allows it, "at least one") —
/// nothing about the panic needs a second mod, and it keeps the fixture
/// to the minimum shape that reproduces it.
#[must_use]
pub fn whole_def_patch_collision_fixture() -> WholeDefPatchCollisionFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");

    let core_locator = locator("core_widget.xml", 0);
    let a_locator = locator("a_patch.xml", 0);

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
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Widget".to_string(),
            Selector::DefName,
        ),
        vec![IndexedPatchOp {
            mod_id: mod_a.clone(),
            op: PatchOp {
                injected_template_names: std::collections::BTreeSet::new(),
                class: "PatchOperationAdd".to_string(),
                xpath: Some(r#"Defs/ThingDef[defName="Widget"]"#.to_string()),
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
                locator: a_locator.clone(),
            },
        }],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><label>widget</label></ThingDef>".to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]</xpath>
             <value><techLevel>Industrial</techLevel></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod"])
        .build();

    WholeDefPatchCollisionFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`unsupported_op_patch_collision_fixture`] builds.
pub struct UnsupportedOpFixture {
    /// A [`Report`] naming `core.mod`, `c.mod`, `x.mod`, and `d.mod` (in
    /// that load order).
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Wall` and
    /// three patchers' top-level operations, `x.mod`'s an unknown custom
    /// class mid-sequence.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `c.mod` patches `description` (succeeds), `x.mod` then ships an
/// unrecognized custom `Class` (genuinely `Unsupported` — an unknown
/// class name with its own `<xpath>`, never a root-add shape), and
/// `d.mod` would patch `label` — the collision's own contested field —
/// but its op never runs: the unsupported-op fixture.
/// `mods = {c.mod, d.mod}` is this finding's own data, chosen
/// deliberately as an artificial pairing: `c.mod`'s own op targets a
/// *different* field entirely (so its row surely populates *before* the
/// stopper) while `d.mod`'s targets `label` (so *its* row surely
/// doesn't) — a real collision has both mods racing on the same field,
/// but that would make it impossible to also prove a before-the-stopper
/// row survives.
#[must_use]
pub fn unsupported_op_patch_collision_fixture() -> UnsupportedOpFixture {
    let core = ModId::new("core.mod");
    let mod_c = ModId::new("c.mod");
    let mod_x = ModId::new("x.mod");
    let mod_d = ModId::new("d.mod");

    let core_locator = locator("core_wall.xml", 0);
    let c_locator = locator("c_patch.xml", 0);
    let x_locator = locator("x_patch.xml", 0);
    let d_locator = locator("d_patch.xml", 0);

    let mut sources = SourceIndex::default();
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
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_c,
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/description"#,
                c_locator.clone(),
            ),
            make_op(
                &mod_x,
                "SomeThirdParty.WeirdOperation",
                r#"Defs/ThingDef[defName="Wall"]/fillPercent"#,
                x_locator.clone(),
            ),
            make_op(
                &mod_d,
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/label"#,
                d_locator.clone(),
            ),
        ],
    );

    // `label` is deliberately absent from the raw winner: `d.mod`'s own
    // op (which would create it) never runs past `x.mod`'s stopper, so
    // `label` must have *no* provenance entry at all — not even the
    // initial `Owner(winner)` attribution every raw leaf otherwise starts
    // with — proving its row's `in_game` is genuinely `None`, not merely
    // unset.
    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Wall</defName>\
         <description>a wall</description><fillPercent>1.0</fillPercent></ThingDef>"
            .to_string(),
    );
    elements.insert(
        c_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Wall"]/description</xpath>
             <value><description>a sturdy wall</description></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        x_locator,
        r#"<Operation Class="SomeThirdParty.WeirdOperation">
             <xpath>Defs/ThingDef[defName="Wall"]/fillPercent</xpath>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        d_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Wall"]/label</xpath>
             <value><label>reinforced wall</label></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("c.mod")
        .mod_("x.mod")
        .mod_("d.mod")
        .patch_collision("ThingDef", "Wall", &["c.mod", "d.mod"])
        .build();

    UnsupportedOpFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`contested_field_patched_before_stopper_fixture`] builds.
pub struct ContestedFieldBeforeStopperFixture {
    /// A [`Report`] naming `core.mod`, `c.mod`, `x.mod`, and `y.mod` (in
    /// that load order).
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Wall` and
    /// three patchers' top-level operations, `x.mod`'s an unknown custom
    /// class mid-sequence.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `c.mod` successfully replaces `ThingDef/Wall`'s own `label` — the
/// collision's own contested field — *before* `x.mod`'s unknown custom op
/// stops the fold on an unrelated field (`fillPercent`); `y.mod`, the
/// collision's *other* mod, would also replace `label` but never gets to
/// run. Unlike [`unsupported_op_patch_collision_fixture`] (deliberately
/// artificial: its two `mods` target two *different* fields, so a
/// before/after-the-stopper row can be told apart at all), this fixture's
/// `mods = {c.mod, y.mod}` both genuinely target `label` — the shape a
/// real `PatchCollision`'s own `mods` always have: the collision's own
/// contested field must always come back its
/// own `Conflict` row, never mis-kinded as a clean, single-mod context
/// row just because one contributor reached it before the (unrelated)
/// stopper poisoned the whole preview.
#[must_use]
pub fn contested_field_patched_before_stopper_fixture() -> ContestedFieldBeforeStopperFixture {
    let core = ModId::new("core.mod");
    let mod_c = ModId::new("c.mod");
    let mod_x = ModId::new("x.mod");
    let mod_y = ModId::new("y.mod");

    let core_locator = locator("core_wall2.xml", 0);
    let c_locator = locator("c2_patch.xml", 0);
    let x_locator = locator("x2_patch.xml", 0);
    let y_locator = locator("y2_patch.xml", 0);

    let mut sources = SourceIndex::default();
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
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &mod_c,
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/label"#,
                c_locator.clone(),
            ),
            make_op(
                &mod_x,
                "SomeThirdParty.WeirdOperation",
                r#"Defs/ThingDef[defName="Wall"]/fillPercent"#,
                x_locator.clone(),
            ),
            make_op(
                &mod_y,
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/label"#,
                y_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Wall</defName>\
         <label>a wall</label><fillPercent>1.0</fillPercent></ThingDef>"
            .to_string(),
    );
    elements.insert(
        c_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Wall"]/label</xpath>
             <value><label>sturdy wall</label></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        x_locator,
        r#"<Operation Class="SomeThirdParty.WeirdOperation">
             <xpath>Defs/ThingDef[defName="Wall"]/fillPercent</xpath>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        y_locator,
        r#"<Operation Class="PatchOperationReplace">
             <xpath>Defs/ThingDef[defName="Wall"]/label</xpath>
             <value><label>reinforced wall</label></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("c.mod")
        .mod_("x.mod")
        .mod_("y.mod")
        .patch_collision("ThingDef", "Wall", &["c.mod", "y.mod"])
        .build();

    ContestedFieldBeforeStopperFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
