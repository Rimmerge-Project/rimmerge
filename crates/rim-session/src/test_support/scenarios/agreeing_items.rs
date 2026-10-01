//! Same-identity list-item scenarios where two or more contributors agree (or one differs).

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{DefEntry, ModId, PatchOp, Report, Selector, XmlLocator};

use crate::test_support::InMemoryDefSourceReader;
use crate::test_support::locator;

/// Everything [`same_identity_list_item_fixture`] builds.
pub struct SameIdentityListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// both patchers' `comps`-adding operations, both under the *same*
    /// declared `li` identity.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `b.mod` each add their own `li Class="CompProperties_Alpha"`
/// item to `ThingDef/Widget`'s `comps` — the *same* declared identity,
/// different children. `rim_merge::effective`'s own `li` identification
/// (`crate::tree::identify_all_li`) can't tell same-identity siblings in
/// one container apart, so both fall back to `ItemId::Position`
/// (`comps/li[#0]`/`comps/li[#1]`) — a fixture pinning how
/// `def_conflict_view` attributes same-identity positional entries on top
/// of `rim-merge`'s own identity fallback.
#[must_use]
pub fn same_identity_list_item_fixture() -> SameIdentityListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget2.xml", 0);
    let a_locator = locator("a5_patch.xml", 0);
    let b_locator = locator("b5_patch.xml", 0);

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
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"><amount>1</amount></li></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"><amount>2</amount></li></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    SameIdentityListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`same_identity_agreeing_list_item_fixture`] builds.
pub struct SameIdentityAgreeingListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// both patchers' `comps`-adding operations, both under the *same*
    /// declared `li` identity with *byte-identical* children.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `b.mod` each add the *exact same* `li Class="CompProperties_Alpha"`
/// item — same identity, byte-identical children — to `ThingDef/Widget`'s
/// `comps`: the "list case" dedup's own two-mod agreement fixture (sibling
/// of [`same_identity_list_item_fixture`]'s differing-content
/// one). `rim_merge::effective`'s own duplicate-identity fallback still
/// numbers both entries by position (`comps/li[#0]`/`comps/li[#1]`), but
/// `rim_merge::effective::duplicate_of_earlier_sibling` recognizes the
/// second as a byte-identical duplicate of the first, so
/// `Session::def_conflict_view` folds them into one `ListEntry` row
/// (`a.mod`'s own, `FieldRow::agreed_by: [b.mod]`) instead of two
/// indistinguishable-looking ones.
#[must_use]
pub fn same_identity_agreeing_list_item_fixture() -> SameIdentityAgreeingListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget3.xml", 0);
    let a_locator = locator("a6_patch.xml", 0);
    let b_locator = locator("b6_patch.xml", 0);

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
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    let identical_add = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"><amount>1</amount></li></value>
           </Operation>"#
        .to_string();
    elements.insert(a_locator, identical_add.clone());
    elements.insert(b_locator, identical_add);

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    SameIdentityAgreeingListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`three_mod_agreeing_list_item_fixture`] builds.
pub struct ThreeModAgreeingListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, `b.mod`, and `c.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// all three patchers' `comps`-adding operations, all under the *same*
    /// declared `li` identity with *byte-identical* children.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod`, `b.mod`, and `c.mod` each add the *exact same*
/// `li Class="CompProperties_Alpha"` item to `ThingDef/Widget`'s `comps` —
/// the "list case" dedup's own three-mod agreement fixture: every entry
/// past the first folds into one `ListEntry` row (`a.mod`'s own,
/// `FieldRow::agreed_by: [b.mod, c.mod]`), proving the fold isn't limited
/// to a single pair.
#[must_use]
pub fn three_mod_agreeing_list_item_fixture() -> ThreeModAgreeingListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let mod_c = ModId::new("c.mod");

    let core_locator = locator("core_widget4.xml", 0);
    let a_locator = locator("a7_patch.xml", 0);
    let b_locator = locator("b7_patch.xml", 0);
    let c_locator = locator("c7_patch.xml", 0);

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
            make_add(
                &mod_c,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                c_locator.clone(),
            ),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    let identical_add = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Alpha"><amount>1</amount></li></value>
           </Operation>"#
        .to_string();
    elements.insert(a_locator, identical_add.clone());
    elements.insert(b_locator, identical_add.clone());
    elements.insert(c_locator, identical_add);

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod", "c.mod"])
        .build();

    ThreeModAgreeingListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`owner_agreeing_list_item_fixture`] builds.
pub struct OwnerAgreeingListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` — a
    /// non-empty `comps` declaring one `li` itself — and both patchers'
    /// `comps`-adding operations.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `core.mod`'s own raw `comps` already declares
/// `<li Class="CompProperties_Base"><amount>1</amount></li>`; `a.mod` adds
/// the *exact same* item (same identity, byte-identical children); `b.mod`
/// adds a genuinely distinct one. `rim_merge::effective::duplicate_of_earlier_sibling`
/// finds `a.mod`'s byte-identical match in `core.mod`'s own
/// [`rim_merge::effective::Provenance::Owner`] entry, which
/// `context_rows_from_provenance`'s own `Provenance::Patch`-and-`mods`-only
/// membership rule never turns into a row of its own, so folding into it
/// would silently drop `a.mod`'s row with nothing left to attach
/// `agreed_by` to. `a.mod` must get its own ordinary `ListEntry` row (the
/// base's own duplicate content is simply invisible here, exactly as an
/// owner-declared field always is in this fold) and `b.mod` its own
/// separate one.
#[must_use]
pub fn owner_agreeing_list_item_fixture() -> OwnerAgreeingListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget6.xml", 0);
    let a_locator = locator("a10_patch.xml", 0);
    let b_locator = locator("b10_patch.xml", 0);

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
        r#"<ThingDef><defName>Widget</defName><comps><li Class="CompProperties_Base"><amount>1</amount></li></comps></ThingDef>"#
            .to_string());
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Base"><amount>1</amount></li></value>
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

    OwnerAgreeingListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`out_of_scope_patcher_agreeing_list_item_fixture`] builds.
pub struct OutOfScopePatcherAgreeingListItemFixture {
    /// A [`Report`] naming `core.mod`, `x.mod`, `a.mod`, and `b.mod` — the
    /// collision's own `mods` names only `a.mod`/`b.mod`; `x.mod` is a real,
    /// active patcher of the same def, just not a party to *this*
    /// collision.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` (empty
    /// `comps`) and all three patchers' `comps`-adding operations, `x.mod`
    /// loading before the other two.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `x.mod` adds `<li Class="CompProperties_Base"><amount>1</amount></li>`
/// to `ThingDef/Widget`'s `comps`; `a.mod` (a party to *this* collision)
/// then adds the exact same item; `b.mod` (also a party) adds a distinct
/// one. A byte-identical earlier sibling can be a
/// [`rim_merge::effective::Provenance::Patch`] too, just by a mod outside
/// the collision's own `mods` — `a.mod`'s own row must not vanish the way
/// it would against an owner-declared item, since
/// `context_rows_from_provenance` never turns `x.mod`'s own entry into a
/// row either (`mods` excludes it). `a.mod` gets its own row.
#[must_use]
pub fn out_of_scope_patcher_agreeing_list_item_fixture() -> OutOfScopePatcherAgreeingListItemFixture
{
    let core = ModId::new("core.mod");
    let mod_x = ModId::new("x.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget7.xml", 0);
    let x_locator = locator("x11_patch.xml", 0);
    let a_locator = locator("a11_patch.xml", 0);
    let b_locator = locator("b11_patch.xml", 0);

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
                &mod_x,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                x_locator.clone(),
            ),
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
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Base"><amount>1</amount></li></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Base"><amount>1</amount></li></value>
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
        .mod_("x.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    OutOfScopePatcherAgreeingListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`three_mod_two_agree_one_differs_list_item_fixture`] builds.
pub struct ThreeModTwoAgreeOneDiffersListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, `b.mod`, and `c.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// all three patchers' `comps`-adding operations, all under the same
    /// declared identity — `a.mod`/`c.mod` byte-identical, `b.mod`
    /// distinct content.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `c.mod` each add the exact same
/// `li Class="CompProperties_Shared"` item; `b.mod` (loading between them)
/// adds one under the same identity but genuinely different content — the
/// "two agree, one differs" case:
/// `a.mod`'s row folds `c.mod` into its own `agreed_by` (the two byte-
/// identical entries), while `b.mod` keeps its own separate row untouched
/// (nothing else matches its own differing content).
#[must_use]
pub fn three_mod_two_agree_one_differs_list_item_fixture()
-> ThreeModTwoAgreeOneDiffersListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");
    let mod_c = ModId::new("c.mod");

    let core_locator = locator("core_widget8.xml", 0);
    let a_locator = locator("a12_patch.xml", 0);
    let b_locator = locator("b12_patch.xml", 0);
    let c_locator = locator("c12_patch.xml", 0);

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
            make_add(
                &mod_c,
                r#"Defs/ThingDef[defName="Widget"]/comps"#,
                c_locator.clone(),
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
             <value><li Class="CompProperties_Shared"><amount>1</amount></li></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Shared"><amount>2</amount></li></value>
           </Operation>"#
            .to_string(),
    );
    elements.insert(
        c_locator,
        r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li Class="CompProperties_Shared"><amount>1</amount></li></value>
           </Operation>"#
            .to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod", "c.mod"])
        .build();

    ThreeModTwoAgreeOneDiffersListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}

/// Everything [`identity_less_agreeing_list_item_fixture`] builds.
pub struct IdentityLessAgreeingListItemFixture {
    /// A [`Report`] naming `core.mod`, `a.mod`, and `b.mod`.
    pub report: Report,
    /// A [`SourceIndex`] locating `core.mod`'s own `ThingDef/Widget` and
    /// both patchers' `comps`-adding operations, both a byte-identical,
    /// identity-less `li`.
    pub sources: SourceIndex,
    /// A reader seeded with every element `sources`' locators name.
    pub reader: InMemoryDefSourceReader,
}

/// `a.mod` and `b.mod` each add the exact same identity-less
/// `<li><amount>1</amount></li>` (no `Class`, no well-known key child, no
/// text — `ItemIdentity::identify` falls straight to `ItemId::Position`
/// for both, the same fallback `identity_less_list_items_fixture` already
/// exercises for *differing* content) to `ThingDef/Widget`'s `comps`. A
/// stated decision: the "list case" dedup applies here
/// too, exactly as it does for a genuine declared-identity collision —
/// [`rim_merge::effective::duplicate_of_earlier_sibling`] only ever
/// compares real content at a `Position`-identified path, never *why* that
/// path fell back to `Position` in the first place, so two mods
/// independently declaring the exact same identity-less item is folded
/// into one row the same way two mods sharing a `Class` are.
#[must_use]
pub fn identity_less_agreeing_list_item_fixture() -> IdentityLessAgreeingListItemFixture {
    let core = ModId::new("core.mod");
    let mod_a = ModId::new("a.mod");
    let mod_b = ModId::new("b.mod");

    let core_locator = locator("core_widget9.xml", 0);
    let a_locator = locator("a13_patch.xml", 0);
    let b_locator = locator("b13_patch.xml", 0);

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
    elements.insert(
        core_locator,
        "<ThingDef><defName>Widget</defName><comps></comps></ThingDef>".to_string(),
    );
    let identical_add = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[defName="Widget"]/comps</xpath>
             <value><li><amount>1</amount></li></value>
           </Operation>"#
        .to_string();
    elements.insert(a_locator, identical_add.clone());
    elements.insert(b_locator, identical_add);

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("a.mod")
        .mod_("b.mod")
        .patch_collision("ThingDef", "Widget", &["a.mod", "b.mod"])
        .build();

    IdentityLessAgreeingListItemFixture {
        report,
        sources,
        reader: InMemoryDefSourceReader::new(elements),
    }
}
