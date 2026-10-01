//! Tests for the change inventory.

use rim_analyzer::domain::{
    DefEntry, GeneratedKind, GeneratedMarker, PatchOp, TemplateEntry, TextureOverride,
};

use super::*;
use crate::test_support::locator;
use rim_analyzer::domain::{Conflict, DuplicateTemplateName};
use rim_resolve::domain::FindingKey;
use std::collections::BTreeSet;

fn def_entry(def_type: &str, def_name: &str, parent_name: Option<&str>) -> DefEntry {
    DefEntry {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: parent_name.map(str::to_string),
        locator: locator(&format!("{def_type}_{def_name}.xml"), 0),
    }
}

fn template_entry(def_type: &str, name: &str) -> TemplateEntry {
    TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: def_type.to_string(),
        name: name.to_string(),
        parent_name: None,
        is_abstract: true,
        locator: locator(&format!("{def_type}_{name}_template.xml"), 0),
    }
}

fn top_level_op(
    def_type: &str,
    def_name: &str,
    mod_id: &ModId,
) -> rim_analyzer::analysis::IndexedPatchOp {
    rim_analyzer::analysis::IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: Some(format!(
                r#"Defs/{def_type}[defName="{def_name}"]/statBases"#
            )),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            // A single-ordinal locator — a genuine top-level `<Operation>`
            // per `IndexedPatchOp::is_top_level`'s own doc comment.
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
            locator: locator(&format!("{def_type}_{def_name}_patch.xml"), 0),
        },
    }
}

/// A mod (`owner.mod`) that:
/// - owns `ThingDef/Wall`, contested by `rival.mod` (a real
///   `Conflict::DefOverride`) and patched by `patcher.mod`;
/// - owns `ThingDef/Door` outright, uncontested;
/// - registers the `ThingDef/@Base` template, with `child.mod`'s
///   `ThingDef/Reinforced` deriving from it;
/// - patches `third.mod`'s `ThingDef/Roof`, which nobody else touches;
/// - ships `Textures/x.png`, also shipped by `texture-rival.mod` (a
///   real `Conflict::TextureOverride`).
fn owner_fixture() -> (SourceIndex, Report) {
    let owner = ModId::new("owner.mod");
    let rival = ModId::new("rival.mod");
    let patcher = ModId::new("patcher.mod");
    let third = ModId::new("third.mod");
    let child = ModId::new("child.mod");
    let texture_rival = ModId::new("texture-rival.mod");

    let mut sources = SourceIndex::default();

    // `ThingDef/Wall`: owned by `owner`/`rival`, patched by `patcher`.
    let wall_key = ("ThingDef".to_string(), "Wall".to_string());
    sources.defs.insert(
        (owner.clone(), wall_key.clone()),
        vec![def_entry("ThingDef", "Wall", None)],
    );
    sources.defs.insert(
        (rival.clone(), wall_key.clone()),
        vec![def_entry("ThingDef", "Wall", None)],
    );
    sources
        .owners_by_def
        .insert(wall_key.clone(), vec![owner.clone(), rival.clone()]);
    sources.patch_ops_by_def.insert(
        (wall_key.0.clone(), wall_key.1.clone(), Selector::DefName),
        vec![top_level_op("ThingDef", "Wall", &patcher)],
    );
    sources
        .ops_by_mod
        .entry(patcher.clone())
        .or_default()
        .insert(
            (wall_key.0.clone(), wall_key.1.clone(), Selector::DefName),
            1,
        );

    // `ThingDef/Door`: owned solely by `owner`, untouched by anyone else.
    let door_key = ("ThingDef".to_string(), "Door".to_string());
    sources.defs.insert(
        (owner.clone(), door_key.clone()),
        vec![def_entry("ThingDef", "Door", None)],
    );
    sources.owners_by_def.insert(door_key, vec![owner.clone()]);

    // `ThingDef/@Base`: registered by `owner`; `child`'s `Reinforced`
    // derives from it.
    let base_key = ("ThingDef".to_string(), "Base".to_string());
    sources.templates.insert(
        base_key.clone(),
        vec![(owner.clone(), template_entry("ThingDef", "Base"))],
    );
    let reinforced_key = ("ThingDef".to_string(), "Reinforced".to_string());
    sources.defs.insert(
        (child.clone(), reinforced_key.clone()),
        vec![def_entry("ThingDef", "Reinforced", Some("Base"))],
    );
    sources
        .owners_by_def
        .insert(reinforced_key.clone(), vec![child.clone()]);
    sources
        .children_by_template
        .insert(base_key, vec![(child.clone(), reinforced_key)]);

    // `ThingDef/Roof`: owned solely by `third`, patched only by `owner`.
    let roof_key = ("ThingDef".to_string(), "Roof".to_string());
    sources.defs.insert(
        (third.clone(), roof_key.clone()),
        vec![def_entry("ThingDef", "Roof", None)],
    );
    sources
        .owners_by_def
        .insert(roof_key.clone(), vec![third.clone()]);
    sources.patch_ops_by_def.insert(
        (roof_key.0.clone(), roof_key.1.clone(), Selector::DefName),
        vec![top_level_op("ThingDef", "Roof", &owner)],
    );
    sources.ops_by_mod.entry(owner.clone()).or_default().insert(
        (roof_key.0.clone(), roof_key.1.clone(), Selector::DefName),
        1,
    );

    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("rival.mod")
        .mod_("patcher.mod")
        .mod_("third.mod")
        .mod_("child.mod")
        .mod_("texture-rival.mod")
        .def_override("ThingDef", "Wall", &["owner.mod", "rival.mod"])
        .build();
    report
        .conflicts
        .push(Conflict::TextureOverride(TextureOverride {
            texture_path: "Textures/x.png".to_string(),
            owners: vec![owner.clone(), texture_rival.clone()],
            same_author: false,
        }));

    (sources, report)
}

fn def_ref(def_type: &str, def_name: &str, selector: Selector) -> DefRef {
    DefRef::new(
        DefKey {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
        },
        selector,
    )
}

#[test]
fn an_owner_row_counts_other_owners_and_patchers_and_carries_the_conflicts_finding() {
    let (sources, report) = owner_fixture();

    let page = query(
        &ModId::new("owner.mod"),
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let wall = page
        .items
        .iter()
        .find(|row| row.def_ref.as_ref() == Some(&def_ref("ThingDef", "Wall", Selector::DefName)))
        .expect("Wall must be an OwnsDef row");
    assert_eq!(wall.kind, ChangeKind::OwnsDef);
    assert_eq!(wall.other_touchers, 2, "rival (owner) + patcher");
    assert_eq!(wall.op_count, 0, "owner never patches its own Wall");
    assert_eq!(
        wall.finding_keys,
        vec![FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string()
            },
            owners: [ModId::new("owner.mod"), ModId::new("rival.mod")]
                .into_iter()
                .collect(),
        }]
    );

    let door = page
        .items
        .iter()
        .find(|row| row.def_ref.as_ref() == Some(&def_ref("ThingDef", "Door", Selector::DefName)))
        .expect("Door must be an OwnsDef row");
    assert_eq!(door.other_touchers, 0);
    assert_eq!(door.op_count, 0);
    assert!(
        door.finding_keys.is_empty(),
        "an uncontested def has no finding"
    );
}

#[test]
fn a_template_registrar_row_counts_its_child() {
    let (sources, report) = owner_fixture();

    let page = query(
        &ModId::new("owner.mod"),
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let base = page
        .items
        .iter()
        .find(|row| row.kind == ChangeKind::OwnsTemplate)
        .expect("Base must be an OwnsTemplate row");
    assert_eq!(
        base.def_ref,
        Some(def_ref("ThingDef", "Base", Selector::NameAttr))
    );
    assert_eq!(
        base.other_touchers, 1,
        "child.mod's Reinforced derives from it"
    );
    assert_eq!(base.op_count, 0);
    assert!(base.finding_keys.is_empty(), "only one registrant");
}

#[test]
fn a_patcher_row_names_the_foreign_owner_with_no_finding_when_uncontested() {
    let (sources, report) = owner_fixture();

    let page = query(
        &ModId::new("owner.mod"),
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let roof = page
        .items
        .iter()
        .find(|row| row.kind == ChangeKind::PatchesDef)
        .expect("Roof must be a PatchesDef row");
    assert_eq!(
        roof.def_ref,
        Some(def_ref("ThingDef", "Roof", Selector::DefName))
    );
    assert_eq!(roof.other_touchers, 1, "third.mod, the sole owner");
    assert_eq!(roof.op_count, 1, "owner's own top-level op on Roof");
    assert!(
        roof.finding_keys.is_empty(),
        "a single patcher is not a collision"
    );
}

#[test]
fn an_asset_overrider_row_carries_the_conflicts_finding() {
    let (sources, report) = owner_fixture();

    let page = query(
        &ModId::new("owner.mod"),
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let texture = page
        .items
        .iter()
        .find(|row| row.kind == ChangeKind::OverridesAsset(AssetKind::Texture))
        .expect("the shared texture must be an OverridesAsset row");
    assert_eq!(texture.def_ref, None);
    assert_eq!(texture.asset_path.as_deref(), Some("Textures/x.png"));
    assert_eq!(texture.other_touchers, 1);
    assert_eq!(texture.op_count, 0);
    assert_eq!(
        texture.finding_keys,
        vec![FindingKey::TextureOverride {
            texture_path: "Textures/x.png".to_string(),
            owners: [ModId::new("owner.mod"), ModId::new("texture-rival.mod")]
                .into_iter()
                .collect(),
        }]
    );
}

#[test]
fn rows_are_ordered_by_other_touchers_desc_then_kind_then_def_ref() {
    let (sources, report) = owner_fixture();

    let page = query(
        &ModId::new("owner.mod"),
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let kinds: Vec<ChangeKind> = page.items.iter().map(|row| row.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ChangeKind::OwnsDef,                            // Wall, 2 touchers
            ChangeKind::OwnsTemplate,                       // Base, 1 toucher
            ChangeKind::PatchesDef,                         // Roof, 1 toucher
            ChangeKind::OverridesAsset(AssetKind::Texture), // texture, 1 toucher
            ChangeKind::OwnsDef,                            // Door, 0 touchers
        ],
        "expected other_touchers-desc, then kind, then def_ref/asset_path ordering"
    );
    assert_eq!(page.total, 5);
    assert_eq!(page.kind_counts.get(&ChangeKind::OwnsDef), Some(&2));
    assert_eq!(page.kind_counts.get(&ChangeKind::OwnsTemplate), Some(&1));
    assert_eq!(page.kind_counts.get(&ChangeKind::PatchesDef), Some(&1));
    assert_eq!(
        page.kind_counts
            .get(&ChangeKind::OverridesAsset(AssetKind::Texture)),
        Some(&1)
    );
}

#[test]
fn paging_returns_a_stable_slice_of_the_same_order() {
    let (sources, report) = owner_fixture();
    let target = ModId::new("owner.mod");

    let full = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );
    let page = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            offset: 1,
            limit: 2,
            ..ChangeFilter::default()
        },
    );

    assert_eq!(page.total, 5);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items, full.items[1..3]);
}

#[test]
fn search_matches_the_def_ref_or_asset_path_case_insensitively() {
    let (sources, report) = owner_fixture();
    let target = ModId::new("owner.mod");

    let page = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            search: Some("roof".to_string()),
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].kind, ChangeKind::PatchesDef);
}

#[test]
fn filtering_by_kind_narrows_the_page_and_total() {
    let (sources, report) = owner_fixture();
    let target = ModId::new("owner.mod");

    let page = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            kinds: Some([ChangeKind::OwnsDef].into_iter().collect()),
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    assert_eq!(page.total, 2, "Wall and Door");
    assert!(page.items.iter().all(|row| row.kind == ChangeKind::OwnsDef));
}

#[test]
fn limit_is_capped_at_max_page_size() {
    let mut sources = SourceIndex::default();
    let target = ModId::new("owner.mod");
    for i in 0..MAX_PAGE_SIZE + 10 {
        let key = ("ThingDef".to_string(), format!("Def{i}"));
        sources.defs.insert(
            (target.clone(), key.clone()),
            vec![def_entry("ThingDef", &format!("Def{i}"), None)],
        );
        sources.owners_by_def.insert(key, vec![target.clone()]);
    }
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .build();

    let page = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            limit: 10_000,
            ..ChangeFilter::default()
        },
    );

    assert_eq!(page.total, MAX_PAGE_SIZE + 10);
    assert_eq!(page.items.len(), MAX_PAGE_SIZE);
}

#[test]
fn search_defs_ranks_name_matches_before_type_only_matches() {
    let mut sources = SourceIndex::default();
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("a.mod"), ModId::new("b.mod")],
    );
    sources.owners_by_def.insert(
        ("WallDef".to_string(), "Socket".to_string()),
        vec![ModId::new("c.mod")],
    );
    sources.templates.insert(
        ("HediffDef".to_string(), "WallImplant".to_string()),
        vec![(
            ModId::new("d.mod"),
            template_entry("HediffDef", "WallImplant"),
        )],
    );

    let hits = search("wall", 10, &sources);

    assert_eq!(
        hits,
        vec![
            (def_ref("HediffDef", "WallImplant", Selector::NameAttr), 1),
            (def_ref("ThingDef", "Wall", Selector::DefName), 2),
            (def_ref("WallDef", "Socket", Selector::DefName), 1),
        ],
        "name matches (HediffDef/@WallImplant, ThingDef/Wall) must rank before the \
             type-only match (WallDef/Socket)"
    );
}

#[test]
fn search_defs_returns_nothing_for_an_empty_query() {
    let mut sources = SourceIndex::default();
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("a.mod")],
    );

    assert_eq!(
        search("", 10, &sources),
        Vec::new(),
        "an empty query must not fall back to matching everything"
    );
}

#[test]
fn search_defs_is_capped_at_max_page_size() {
    let mut sources = SourceIndex::default();
    for i in 0..MAX_PAGE_SIZE + 5 {
        sources.owners_by_def.insert(
            ("ThingDef".to_string(), format!("Wall{i}")),
            vec![ModId::new("a.mod")],
        );
    }

    let hits = search("wall", 10_000, &sources);

    assert_eq!(hits.len(), MAX_PAGE_SIZE);
}

/// `SourceIndex`'s `defs`/
/// `ops_by_mod` are keyed by the raw `ModsConfig.xml` id, never the
/// base one, so a query resolving `mod_id` straight down to
/// `mod_id.base()` before indexing would miss a Steam-installed mod's own
/// rows entirely whenever its active id carries the `_steam` suffix.
#[test]
fn a_steam_suffixed_mod_finds_its_own_owned_and_patched_rows() {
    let raw = ModId::new("owner.mod_steam");
    let patcher = ModId::new("patcher.mod");

    let mut sources = SourceIndex::default();

    // Owned outright, keyed by the raw `_steam` id — must still
    // surface as an `OwnsDef` row regardless of which form of the id
    // the caller queries with.
    let wall_key = ("ThingDef".to_string(), "Wall".to_string());
    sources.defs.insert(
        (raw.clone(), wall_key.clone()),
        vec![def_entry("ThingDef", "Wall", None)],
    );
    sources
        .owners_by_def
        .insert(wall_key.clone(), vec![raw.clone()]);

    // The Steam-suffixed mod itself patches a foreign def — must
    // still surface as a `PatchesDef` row with a real `op_count`,
    // not silently vanish because `ops_by_mod` is keyed `raw`, not
    // `raw.base()`.
    let roof_key = ("ThingDef".to_string(), "Roof".to_string());
    sources.defs.insert(
        (patcher.clone(), roof_key.clone()),
        vec![def_entry("ThingDef", "Roof", None)],
    );
    sources
        .owners_by_def
        .insert(roof_key.clone(), vec![patcher.clone()]);
    sources.patch_ops_by_def.insert(
        (roof_key.0.clone(), roof_key.1.clone(), Selector::DefName),
        vec![top_level_op("ThingDef", "Roof", &raw)],
    );
    sources.ops_by_mod.entry(raw.clone()).or_default().insert(
        (roof_key.0.clone(), roof_key.1.clone(), Selector::DefName),
        1,
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod_steam")
        .mod_("patcher.mod")
        .build();

    for query_id in [ModId::new("owner.mod"), ModId::new("owner.mod_steam")] {
        let page = query(
            &query_id,
            &report,
            &sources,
            &ChangeFilter {
                limit: 10,
                ..ChangeFilter::default()
            },
        );

        let wall = page
            .items
            .iter()
            .find(|row| row.kind == ChangeKind::OwnsDef)
            .unwrap_or_else(|| panic!("Wall must be an OwnsDef row when queried as {query_id:?}"));
        assert_eq!(wall.other_touchers, 0);

        let roof = page
            .items
            .iter()
            .find(|row| row.kind == ChangeKind::PatchesDef)
            .unwrap_or_else(|| {
                panic!("Roof must be a PatchesDef row when queried as {query_id:?}")
            });
        assert_eq!(
            roof.op_count, 1,
            "op_count must not vanish for a Steam-installed mod (queried as {query_id:?})"
        );
    }
}

/// `kind_counts` reflects `search` but not `kinds` — a UI's kind chips
/// must keep showing what each kind would total under the current
/// search text even while one chip is selected, not collapse every
/// unselected kind to zero.
#[test]
fn kind_counts_are_computed_after_search_but_before_the_kind_filter() {
    let (sources, report) = owner_fixture();
    let target = ModId::new("owner.mod");

    // "wall" matches only the `ThingDef/Wall` `OwnsDef` row; the kind
    // filter then excludes that very kind, so `items` is empty but
    // `kind_counts` must still report it.
    let page = query(
        &target,
        &report,
        &sources,
        &ChangeFilter {
            search: Some("wall".to_string()),
            kinds: Some([ChangeKind::OwnsTemplate].into_iter().collect()),
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    assert!(
        page.items.is_empty(),
        "the kind filter excludes OwnsDef, the only kind the search matched"
    );
    assert_eq!(
        page.kind_counts.get(&ChangeKind::OwnsDef),
        Some(&1),
        "kind_counts must count the search-matched Wall row despite the kind filter"
    );
    assert_eq!(
        page.kind_counts.get(&ChangeKind::OwnsTemplate),
        None,
        "Base didn't match the search text, so it must not appear in kind_counts either"
    );
}

/// A mod that owns a def and also patches that same def gets no
/// separate `PatchesDef` row (its op count folds into the `OwnsDef`
/// row instead, per `owns`'s own doc comment) — and a generated mod
/// among the *other* patchers still counts toward `other_touchers`:
/// that count is about how contested the target is, not about which
/// of its touchers the ledger would later hide from the findings list.
#[test]
fn an_owner_that_patches_its_own_def_gets_no_patches_def_row() {
    let owner = ModId::new("owner.mod");
    let rival = ModId::new("rival.mod");
    let generated = ModId::new("rimmerge.merge.deadbeef1234");

    let mut sources = SourceIndex::default();
    let wall_key = ("ThingDef".to_string(), "Wall".to_string());
    sources.defs.insert(
        (owner.clone(), wall_key.clone()),
        vec![def_entry("ThingDef", "Wall", None)],
    );
    sources.defs.insert(
        (rival.clone(), wall_key.clone()),
        vec![def_entry("ThingDef", "Wall", None)],
    );
    sources
        .owners_by_def
        .insert(wall_key.clone(), vec![owner.clone(), rival.clone()]);
    sources.patch_ops_by_def.insert(
        (wall_key.0.clone(), wall_key.1.clone(), Selector::DefName),
        vec![
            top_level_op("ThingDef", "Wall", &owner),
            top_level_op("ThingDef", "Wall", &generated),
        ],
    );
    sources.ops_by_mod.entry(owner.clone()).or_default().insert(
        (wall_key.0.clone(), wall_key.1.clone(), Selector::DefName),
        1,
    );
    sources
        .ops_by_mod
        .entry(generated.clone())
        .or_default()
        .insert(
            (wall_key.0.clone(), wall_key.1.clone(), Selector::DefName),
            1,
        );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("rival.mod")
        .mod_with("rimmerge.merge.deadbeef1234", |m| {
            m.generated = Some(GeneratedMarker {
                kind: GeneratedKind::Merge,
                patch_id: None,
                scope: None,
            });
        })
        .build();

    let page = query(
        &owner,
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    assert!(
        page.items
            .iter()
            .all(|row| row.kind != ChangeKind::PatchesDef),
        "the owner's own patch on its own def must not become a PatchesDef row"
    );
    let wall = page
        .items
        .iter()
        .find(|row| row.kind == ChangeKind::OwnsDef)
        .expect("Wall must still be an OwnsDef row");
    assert_eq!(
        wall.op_count, 1,
        "the owner's own self-patch must be counted on the OwnsDef row"
    );
    assert_eq!(
        wall.other_touchers, 2,
        "rival (owner) + the generated patcher, excluding the target itself"
    );
}

/// `rim_analyzer`'s template-owners index (and so this crate's own
/// `ConflictIndex::duplicate_templates`) is keyed by `Name` alone, not
/// `(def_type, Name)` — see `ChangeRow::finding_keys`'s own doc
/// comment. A `DuplicateTemplateName` conflict whose `owners` are, in
/// fact, two raw ids of the very same physical mod (a local copy and
/// its own `_steam` counterpart) names no genuine second party, so it
/// must not be attached to that mod's own `OwnsTemplate` row.
#[test]
fn owns_template_never_attaches_a_duplicate_name_key_with_no_other_base_owner() {
    let owner = ModId::new("owner.mod");
    let owner_steam = ModId::new("owner.mod_steam");

    let mut sources = SourceIndex::default();
    let key = ("ThingDef".to_string(), "Foo".to_string());
    sources.templates.insert(
        key.clone(),
        vec![(owner.clone(), template_entry("ThingDef", "Foo"))],
    );

    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("owner.mod")
        .mod_("owner.mod_steam")
        .build();
    report
        .conflicts
        .push(Conflict::DuplicateTemplateName(DuplicateTemplateName {
            name: "Foo".to_string(),
            owners: vec![owner.clone(), owner_steam.clone()],
        }));

    let page = query(
        &owner,
        &report,
        &sources,
        &ChangeFilter {
            limit: 10,
            ..ChangeFilter::default()
        },
    );

    let foo = page
        .items
        .iter()
        .find(|row| row.kind == ChangeKind::OwnsTemplate)
        .expect("Foo must be an OwnsTemplate row");
    assert!(
        foo.finding_keys.is_empty(),
        "both `owners` share one base mod, so there is no genuine second party"
    );
}
