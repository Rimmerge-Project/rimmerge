//! Tests for the conflict detectors.

use super::patches::collision_severity;
use super::*;
use crate::analysis::indices::{ActiveMods, DisplayNameIndex, Indices};
use crate::domain::{
    AssemblyInfo, DeclaredOrder, DefEntry, DefTarget, Mod, PatchOp, Selector, Source,
    TexturePathCandidate, XmlLocator,
};
use crate::domain::{Conflict, ModsById, PatchCollisionEntry, PatchCollisionSeverity, ScannedMod};
use std::collections::BTreeSet;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

fn def_entry(def_type: &str, def_name: &str) -> DefEntry {
    DefEntry {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }
}

fn mod_with(
    id: &str,
    author: &str,
    defs: Vec<DefEntry>,
    assemblies: Vec<AssemblyInfo>,
) -> ScannedMod {
    mod_with_declared(id, author, defs, assemblies, DeclaredOrder::default())
}

fn mod_with_declared(
    id: &str,
    author: &str,
    defs: Vec<DefEntry>,
    assemblies: Vec<AssemblyInfo>,
    declared: DeclaredOrder,
) -> ScannedMod {
    ScannedMod {
        info: Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: vec![author.to_string()],
            url: None,
            path: PathBuf::from(id),
            source: Source::Local,
            supported_versions: Vec::new(),
            declared,
            loaded_folders: Vec::new(),
            hard_dependents: 0,
            soft_dependents: 0,
            awareness_dependents: 0,
            is_framework_candidate: false,
            generated: None,
            workshop_id: None,
            load_folders_version_matched: None,
        },
        defs,
        templates: Vec::new(),
        patch_ops: Vec::new(),
        textures: std::collections::BTreeMap::new(),
        assemblies,
        sounds: std::collections::BTreeSet::new(),
        translation_keys: std::collections::BTreeSet::new(),
        inline_types: std::collections::BTreeSet::new(),
        manifest_order: Default::default(),
        texture_path_candidates: Vec::new(),
        inline_node_path_hashes: std::collections::HashSet::new(),
        if_mod_active_targets: Vec::new(),
        scan_cost: crate::domain::ScanCost::default(),
        nameless_def_count: 0,
        bundle_textures: Default::default(),
        undecodable_textures: Vec::new(),
        nested_may_require: Vec::new(),
    }
}

fn build_indices(scanned: &[ScannedMod]) -> Indices {
    Indices::build(
        scanned,
        &HashSet::new(),
        &active_of(scanned),
        &large_core_resource_index(),
    )
}

/// A synthetic Core resource index large enough to clear
/// [`super::indices::MIN_CORE_RESOURCE_TEXTURES`], so `build_indices`
/// exercises `missing_texture_path`'s ordinary resolution logic rather than
/// its "too few entries" self-disabling gate. Every key lives under a
/// reserved namespace no test's own candidate path ever names, so it can
/// never accidentally rescue (or collide with) a real assertion.
fn large_core_resource_index() -> BTreeSet<String> {
    (0..crate::analysis::indices::MIN_CORE_RESOURCE_TEXTURES)
        .map(|i| format!("unittest_padding/{i}"))
        .collect()
}

fn active_of(scanned: &[ScannedMod]) -> ActiveMods {
    ActiveMods::build(scanned)
}

/// The scanned mods' report entries by id, as the winner-relative facts read them.
fn mods_by_id(scanned: &[ScannedMod]) -> ModsById<'_> {
    scanned
        .iter()
        .map(|sm| (sm.info.id.clone(), &sm.info))
        .collect()
}

fn patch_op(class: &str, target: DefTarget) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        class: class.to_string(),
        xpath: None,
        target: Some(target),
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
        locator: XmlLocator::for_test(),
    }
}

fn def_name_target(def_type: &str, def_name: &str) -> DefTarget {
    DefTarget {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        selector: Selector::DefName,
        sub_path: None,
    }
}

#[test]
fn def_override_winner_is_last_in_load_order() {
    let scanned = vec![
        mod_with("a", "Alice", vec![def_entry("ThingDef", "Wall")], vec![]),
        mod_with("b", "Bob", vec![def_entry("ThingDef", "Wall")], vec![]),
    ];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

    let conflicts = def_overrides(&indices, &load_order, &BTreeMap::new());

    assert_eq!(conflicts.len(), 1);
    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert_eq!(c.owners, vec![ModId::new("b"), ModId::new("a")]);
    assert!(!c.same_author);
    assert!(!c.winner_declares_relation(&ModId::new("a"), &mods_by_id(&scanned)));
    assert!(!c.shadows_framework(&ModId::new("a"), &mods_by_id(&scanned)));
}

/// The winner declaring a `loadAfter` naming every other owner marks
/// this override as intentional, not accidental.
#[test]
fn def_override_winner_declares_relation_when_it_names_every_other_owner() {
    let a = mod_with("a", "Alice", vec![def_entry("ThingDef", "Wall")], vec![]);
    let winner = mod_with_declared(
        "b",
        "Bob",
        vec![def_entry("ThingDef", "Wall")],
        vec![],
        DeclaredOrder {
            load_after: vec![ModId::new("a")],
            ..DeclaredOrder::default()
        },
    );
    let scanned = vec![a, winner];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = def_overrides(&indices, &load_order, &BTreeMap::new());

    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert!(c.winner_declares_relation(&ModId::new("b"), &mods_by_id(&scanned)));
    assert!(!c.winner_declares_relation(&ModId::new("a"), &mods_by_id(&scanned)));
}

/// An earlier owner that's a framework candidate, shadowed by a
/// winner that isn't one itself and declares no relation to it, is
/// flagged as `shadows_framework`.
#[test]
fn def_override_shadows_framework_when_earlier_owner_is_a_framework_candidate() {
    let mut framework = mod_with(
        "framework",
        "Alice",
        vec![def_entry("ThingDef", "Wall")],
        vec![],
    );
    framework.info.is_framework_candidate = true;
    let leaf = mod_with("leaf", "Bob", vec![def_entry("ThingDef", "Wall")], vec![]);
    let scanned = vec![framework, leaf];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("framework"), ModId::new("leaf")]);

    let conflicts = def_overrides(&indices, &load_order, &BTreeMap::new());

    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert!(c.shadows_framework(&ModId::new("leaf"), &mods_by_id(&scanned)));
    assert!(!c.shadows_framework(&ModId::new("framework"), &mods_by_id(&scanned)));
}

#[test]
fn def_override_flags_vanilla_override() {
    let mut core = mod_with(
        "core",
        "Ludeon",
        vec![def_entry("ThingDef", "Wall")],
        vec![],
    );
    core.info.source = Source::Core;
    let modded = mod_with("m", "Modder", vec![def_entry("ThingDef", "Wall")], vec![]);
    let scanned = vec![core, modded];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("core"), ModId::new("m")]);

    let conflicts = def_overrides(&indices, &load_order, &BTreeMap::new());

    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert!(c.overrides_vanilla);
}

#[test]
fn single_owner_is_not_a_conflict() {
    let scanned = vec![mod_with(
        "a",
        "Alice",
        vec![def_entry("ThingDef", "Wall")],
        vec![],
    )];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a")]);
    assert!(def_overrides(&indices, &load_order, &BTreeMap::new()).is_empty());
}

/// A `Defs/`-inline def colliding with a def another mod patch-injects as a
/// whole new def (`injected_def_owners`, not `indices.def_owners` — the
/// injector writes nothing inline) is still reported as a `DefOverride`,
/// merging both ownership sources onto one key.
#[test]
fn def_override_reports_an_inline_def_colliding_with_a_whole_def_patch_injection() {
    let scanned = vec![
        mod_with(
            "inline_owner",
            "Alice",
            vec![def_entry("ThingDef", "NewThing")],
            vec![],
        ),
        mod_with("injector", "Bob", vec![], vec![]),
    ];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("inline_owner"), ModId::new("injector")]);
    let injected_owners = BTreeMap::from([(
        ("ThingDef".to_string(), "NewThing".to_string()),
        vec![ModId::new("injector")],
    )]);

    let conflicts = def_overrides(&indices, &load_order, &injected_owners);

    assert_eq!(conflicts.len(), 1);
    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert_eq!(
        c.owners,
        vec![ModId::new("inline_owner"), ModId::new("injector")]
    );
    assert_eq!(c.owners.last(), Some(&ModId::new("injector")));
}

/// The dedup branch itself — `merged_def_owners`'s "kept once, at its inline
/// position" clause; the sibling test above only covers the disjoint case
/// (every mod in exactly one source). Here `dual` inline-defines
/// `ThingDef/Dup` *and* also carries a (redundant) whole-def injection for
/// the identical key: it must appear exactly once in the merged owner list,
/// at its original inline position, not duplicated.
#[test]
fn def_override_deduplicates_a_mod_present_in_both_inline_and_injected_owners() {
    let scanned = vec![
        mod_with("dual", "Alice", vec![def_entry("ThingDef", "Dup")], vec![]),
        mod_with(
            "other_inline",
            "Bob",
            vec![def_entry("ThingDef", "Dup")],
            vec![],
        ),
    ];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![
        ModId::new("dual"),
        ModId::new("other_inline"),
        ModId::new("other_injected"),
    ]);
    let injected_owners = BTreeMap::from([(
        ("ThingDef".to_string(), "Dup".to_string()),
        vec![ModId::new("dual"), ModId::new("other_injected")],
    )]);

    let conflicts = def_overrides(&indices, &load_order, &injected_owners);

    assert_eq!(conflicts.len(), 1);
    let Conflict::DefOverride(c) = &conflicts[0] else {
        panic!("expected DefOverride")
    };
    assert_eq!(
        c.owners,
        vec![
            ModId::new("dual"),
            ModId::new("other_inline"),
            ModId::new("other_injected"),
        ],
        "`dual` must appear exactly once, at its original inline position"
    );
}

#[test]
fn duplicate_assembly_requires_two_non_vanilla_owners() {
    let mut core = mod_with(
        "core",
        "Ludeon",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    core.info.source = Source::Core;
    let modded = mod_with(
        "m",
        "Modder",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let scanned = vec![core, modded];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("core"), ModId::new("m")]);

    // Only one *mod* ships it (core doesn't count) -> no conflict.
    assert!(duplicate_assemblies(&indices, &load_order).is_empty());
}

#[test]
fn duplicate_assembly_flags_two_mods_shipping_the_same_name() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = duplicate_assemblies(&indices, &load_order);
    assert_eq!(conflicts.len(), 1);
}

/// `versions` lists every owner's own copy version (in load order), and
/// `first_loaded` names the owner that actually loads first — the mod
/// shipping the *older* copy here, since this test applies the given load
/// order as-is rather than the version-precedence edge that would reorder
/// them.
#[test]
fn duplicate_assembly_carries_each_owners_version_and_names_the_first_loaded_owner() {
    let assembly_version = |major: u16| crate::domain::AssemblyVersion {
        major,
        minor: 0,
        build: 0,
        revision: 0,
    };
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: Some(assembly_version(1)),
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: Some(assembly_version(2)),
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = duplicate_assemblies(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::DuplicateAssembly(c) = &conflicts[0] else {
        panic!("expected DuplicateAssembly")
    };
    assert_eq!(c.first_loaded, ModId::new("a"));
    assert_eq!(
        c.versions,
        vec![
            (ModId::new("a"), assembly_version(1)),
            (ModId::new("b"), assembly_version(2)),
        ]
    );
}

/// A mod whose DLL metadata failed to parse contributes
/// [`crate::domain::AssemblyVersion::default`] rather than being
/// dropped from `versions` entirely — the version is unknown, not
/// absent from the conflict.
#[test]
fn duplicate_assembly_defaults_a_parse_failed_owners_version() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: true,
        }],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![AssemblyInfo {
            file_name: "0ExampleLib".into(),
            name: "0examplelib".into(),
            references: vec![],
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = duplicate_assemblies(&indices, &load_order);

    let Conflict::DuplicateAssembly(c) = &conflicts[0] else {
        panic!("expected DuplicateAssembly")
    };
    assert_eq!(
        c.versions,
        vec![
            (ModId::new("a"), crate::domain::AssemblyVersion::default()),
            (ModId::new("b"), crate::domain::AssemblyVersion::default()),
        ]
    );
}

#[test]
fn patch_collision_flags_two_distinct_mods_patching_the_same_target() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        def_name_target("ThingDef", "Wall"),
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "Wall"),
    )];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);
    let name_map = DisplayNameIndex::new();

    let conflicts = patch_collisions(&scanned, &load_order, &active_of(&scanned), &name_map);

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(c) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(c.mods.len(), 2);
}

/// Regression guard: `[@Name="X"]` and `[defName="X"]` are different
/// namespaces (see [`Selector`]) — two mods patching each must not be
/// reported as colliding on the same target just because `X` matches.
#[test]
fn patch_collision_key_distinguishes_name_attr_from_def_name_selector() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        DefTarget {
            def_type: "ThingDef".into(),
            def_name: "WallBase".into(),
            selector: Selector::NameAttr,
            sub_path: None,
        },
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "WallBase"),
    )];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(conflicts.is_empty());
}

/// A head naming several defs patches every one of them, so it must
/// collide with another mod's op on *any* of those defs — not only
/// the first one `PatchOp::target` happens to carry.
#[test]
fn patch_collision_sees_every_def_a_multi_def_head_names() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(r#"Defs/ThingDef[defName="Wall" or defName="Door"]/statBases"#.to_string()),
        ..patch_op(
            "PatchOperationReplace",
            DefTarget {
                def_type: "ThingDef".into(),
                def_name: "Wall".into(),
                selector: Selector::DefName,
                sub_path: Some("statBases".into()),
            },
        )
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(r#"Defs/ThingDef[defName="Door"]/statBases"#.to_string()),
        ..patch_op(
            "PatchOperationAdd",
            DefTarget {
                def_type: "ThingDef".into(),
                def_name: "Door".into(),
                selector: Selector::DefName,
                sub_path: Some("statBases".into()),
            },
        )
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.def_name, "Door");
    assert_eq!(collision.severity, PatchCollisionSeverity::Contested);
}

/// A root predicate on the def node is not part of the collision's
/// `sub_path`: two mods patching `/statBases`, one of them guarded by
/// `[not(comps)]`, contest the same field.
#[test]
fn patch_collision_ignores_root_predicates_when_keying_the_sub_path() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(r#"Defs/ThingDef[defName="Wall"][not(comps)]/statBases"#.to_string()),
        ..patch_op("PatchOperationReplace", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(r#"Defs/ThingDef[defName="Wall"]/statBases"#.to_string()),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path.as_deref(), Some("statBases"));
}

/// Regression guard: without the `is_mutating` filter, a control-flow
/// node (`PatchOperationFindMod`, `PatchOperationSequence`, ...)
/// sharing a target with a real mutation would falsely collide.
#[test]
fn patch_collision_ignores_non_mutating_operations() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        is_mutating: false,
        ..patch_op("PatchOperationFindMod", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "Wall"),
    )];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(conflicts.is_empty());
}

/// Regression guard: without the distinct-mod filter, two mutating
/// ops from the *same* mod on the same target would falsely collide.
#[test]
fn patch_collision_requires_more_than_one_distinct_mod() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![
        patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall")),
        patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall")),
    ];
    let scanned = vec![a];
    let load_order = LoadOrder::new(vec![ModId::new("a")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(conflicts.is_empty());
}

#[test]
fn patch_collision_excludes_op_gated_off_by_unsatisfied_may_require() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        may_require: vec!["not.active".to_string()],
        // A list item is the one shape the game actually reads MayRequire
        // on — a top-level op's own attribute would be unread and
        // this op would run regardless.
        is_list_item: true,
        ..patch_op("PatchOperationReplace", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "Wall"),
    )];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(conflicts.is_empty());
}

/// Two mods each `Add`ing a *different* top-level element to the same def
/// root don't collide — each derives its own `sub_path` from the element it
/// injects.
#[test]
fn patch_collision_split_splits_adds_of_different_children_into_separate_keys() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/statBases".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(
        conflicts.is_empty(),
        "adding different children to the same def root must not collide"
    );
}

/// Two mods `Add`ing the *same* top-level element still collide, keyed by
/// that element's own name rather than the whole def root.
#[test]
fn patch_collision_split_keeps_adds_of_the_same_child_colliding() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path.as_deref(), Some("comps"));
    assert_eq!(collision.mods.len(), 2);
}

/// The "acts on the node itself" case, a Replace at root vs an Add: a
/// `Replace` of the whole def root blocks the split entirely — the `Add`
/// falls back to the same un-split, whole-def key the `Replace` already
/// occupies, so the two land in one bucket rather than the `Add`'s own
/// would-be split key.
#[test]
fn patch_collision_split_replace_at_root_collides_with_an_add_under_it() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        def_name_target("ThingDef", "Wall"),
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path, None);
    assert_eq!(collision.mods.len(), 2);
}

/// An ancestor *union* would report one real whole-def disagreement as N — a
/// `Replace` at the def root unioned into *every* one of that def's split
/// element buckets independently, one contested collision per element,
/// instead of suppressing the split outright. Pins the no-fan-out property
/// directly: a `Replace` at root coexisting with `Add`s of three distinct
/// elements must still yield exactly **one** contested collision, carrying
/// every mod, not three.
#[test]
fn patch_collision_split_a_root_replace_does_not_fan_out_across_several_add_elements() {
    let mut replacer = mod_with("replacer", "Alice", vec![], vec![]);
    replacer.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        def_name_target("ThingDef", "Wall"),
    )];
    let mut adds_comps = mod_with("adds-comps", "Bob", vec![], vec![]);
    adds_comps.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut adds_stat_bases = mod_with("adds-statbases", "Carol", vec![], vec![]);
    adds_stat_bases.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/statBases".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut adds_verbs = mod_with("adds-verbs", "Dan", vec![], vec![]);
    adds_verbs.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/verbs".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![replacer, adds_comps, adds_stat_bases, adds_verbs];
    let load_order = LoadOrder::new(vec![
        ModId::new("replacer"),
        ModId::new("adds-comps"),
        ModId::new("adds-statbases"),
        ModId::new("adds-verbs"),
    ]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(
        conflicts.len(),
        1,
        "one real disagreement must report as one collision, not one per split element"
    );
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path, None);
    assert_eq!(collision.mods.len(), 4);
    assert_eq!(collision.severity, PatchCollisionSeverity::Contested);
}

/// Pins the *widening* failure mode directly, not just blocking's outright
/// removal: `exact_blocked` checked by `(def_type, def_name, selector)`
/// alone, ignoring which `sub_path` the node-acting op actually sits at,
/// would pass every other test while a real install's collision count grew by
/// about a quarter. A `Replace` on an unrelated *sibling* path (`comps`)
/// bears no ancestor relationship to the def root, so it must never block
/// splitting for `Add`s there: the two root-level `Add`s (different elements)
/// must still split apart from each other (no collision between them), and
/// the `Replace` must stay alone in its own `comps` bucket (no collision
/// there either — one contributor only). Any collision at all here is the
/// widened-blocking bug reproduced.
#[test]
fn patch_collision_split_a_replace_on_a_sibling_path_never_blocks_root_level_add_splitting() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        DefTarget {
            def_type: "ThingDef".into(),
            def_name: "Wall".into(),
            selector: Selector::DefName,
            sub_path: Some("comps".into()),
        },
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/statBases".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut c = mod_with("c", "Carol", vec![], vec![]);
    c.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/verbs".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b, c];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(
        conflicts.is_empty(),
        "a Replace on an unrelated sibling path (comps) must never block splitting for \
             root-level Adds at different elements, nor collide with either of them itself — \
             got {conflicts:#?}"
    );
}

/// Pins `acts_on_the_node_itself`'s own `Insert` exclusion directly — it is
/// the only test that fails if `&&
/// !class.to_lowercase().ends_with("patchoperationinsert")` is removed, since
/// nothing else exercises an `Insert` sharing a def with a splittable `Add`.
/// An `Insert` at the def root must never be treated as node-acting: it
/// places its `<value>` as a *sibling* at the xpath location, never touching
/// the def's own content, so it must not block splitting for two `Add`s at
/// the same root injecting different elements.
#[test]
fn patch_collision_split_insert_never_blocks_a_sibling_adds_split() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationInsert",
        def_name_target("ThingDef", "Wall"),
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let mut c = mod_with("c", "Carol", vec![], vec![]);
    c.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/statBases".to_string()]),
        ..patch_op("PatchOperationAdd", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b, c];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert!(
        conflicts.is_empty(),
        "an Insert at the def root must never block a sibling Add's own split, nor collide \
             with either Add itself — got {conflicts:#?}"
    );
}

/// `Insert` never gets the per-element split, even when its `injected_paths`
/// happens to be populated (real extraction never populates it for `Insert` —
/// set directly here only to prove the class gate itself, not the absence of
/// upstream data): two `Insert`s naming different elements still collide at
/// the plain, un-split `sub_path`.
#[test]
fn patch_collision_split_insert_keeps_plain_sub_path_keying() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/comps".to_string()]),
        ..patch_op("PatchOperationInsert", def_name_target("ThingDef", "Wall"))
    }];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/statBases".to_string()]),
        ..patch_op("PatchOperationInsert", def_name_target("ThingDef", "Wall"))
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(
        conflicts.len(),
        1,
        "Insert must never be split by injected element name"
    );
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path, None);
}

/// An `Add` whose injection has no addressable top-level element (its
/// `<value>` yielded nothing `injected_paths` could keep — an empty
/// value, or one an upstream bracket/`li` filter dropped entirely)
/// falls back to the plain `sub_path` key rather than vanishing from
/// collision detection outright.
#[test]
fn patch_collision_split_add_with_no_derivable_element_falls_back_to_sub_path() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "Wall"),
    )];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        def_name_target("ThingDef", "Wall"),
    )];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(
        conflicts.len(),
        1,
        "an unresolvable Add must still collide at the plain sub_path, not disappear"
    );
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path, None);
}

/// The blocking rule isn't root-only: a `Replace` of an existing,
/// non-list sub-element also suppresses splitting for an `Add`
/// targeting that *same* sub-element — the `Add` falls back to the
/// exact same `sub_path` the `Replace` already occupies (never a
/// deeper, split key), so the two collide in one bucket rather than
/// the `Replace` being unioned outward into a separately-split one.
#[test]
fn patch_collision_split_replace_of_a_sub_element_collides_with_an_add_nested_under_it() {
    let target = DefTarget {
        def_type: "ThingDef".into(),
        def_name: "Wall".into(),
        selector: Selector::DefName,
        sub_path: Some("verbProperties".into()),
    };
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.patch_ops = vec![patch_op("PatchOperationReplace", target.clone())];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Wall/verbProperties/warmupTime".to_string()]),
        ..patch_op("PatchOperationAdd", target)
    }];
    let scanned = vec![a, b];
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    assert_eq!(conflicts.len(), 1);
    let Conflict::PatchCollision(collision) = &conflicts[0] else {
        panic!("expected PatchCollision")
    };
    assert_eq!(collision.sub_path.as_deref(), Some("verbProperties"));
    assert_eq!(collision.mods.len(), 2);
}

/// Regression guard: without the distinct-normalized-path grouping in
/// `texture_overrides`, two owners of the same path would be missed.
#[test]
fn texture_override_requires_more_than_one_owner() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.textures.insert("things/wall".to_string(), 0);
    let scanned = vec![a];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a")]);

    assert!(texture_overrides(&indices, &load_order).is_empty());
}

#[test]
fn texture_override_flags_two_mods_shipping_the_same_normalized_path() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.textures.insert("things/wall".to_string(), 0);
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.textures.insert("things/wall".to_string(), 0);
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = texture_overrides(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::TextureOverride(c) = &conflicts[0] else {
        panic!("expected TextureOverride")
    };
    assert_eq!(c.owners.len(), 2);
}

fn collision_entry(mod_id: &str, op_class: &str) -> PatchCollisionEntry {
    PatchCollisionEntry {
        mod_id: ModId::new(mod_id),
        op_class: op_class.to_string(),
    }
}

#[test]
fn two_adds_from_different_mods_is_additive() {
    let entries = vec![
        collision_entry("a", "PatchOperationAdd"),
        collision_entry("b", "PatchOperationAdd"),
    ];
    assert_eq!(
        collision_severity(&entries),
        PatchCollisionSeverity::Additive
    );
}

#[test]
fn add_and_replace_from_different_mods_is_contested() {
    let entries = vec![
        collision_entry("a", "PatchOperationAdd"),
        collision_entry("b", "PatchOperationReplace"),
    ];
    assert_eq!(
        collision_severity(&entries),
        PatchCollisionSeverity::Contested
    );
}

/// Two replaces from the *same* mod aren't contested — there's no
/// other mod's ordering to contest against.
#[test]
fn two_replaces_from_the_same_mod_is_additive() {
    let entries = vec![
        collision_entry("a", "PatchOperationReplace"),
        collision_entry("a", "PatchOperationReplace"),
    ];
    assert_eq!(
        collision_severity(&entries),
        PatchCollisionSeverity::Additive
    );
}

/// A custom, unrecognized op class from a different mod than another
/// mutating op is contested — it could do anything to the target.
#[test]
fn unknown_custom_class_is_contested() {
    let entries = vec![
        collision_entry("a", "PatchOperationAdd"),
        collision_entry("b", "Example.PatchOperationAddOrReplace"),
    ];
    assert_eq!(
        collision_severity(&entries),
        PatchCollisionSeverity::Contested
    );
}

fn def_entries(names: &[&str]) -> Vec<DefEntry> {
    names.iter().map(|n| def_entry("ThingDef", n)).collect()
}

#[test]
fn likely_duplicate_flagged_when_overlap_is_large_and_no_relation_declared() {
    let names: Vec<String> = (0..15).map(|i| format!("Def{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let a = mod_with("a", "Alice", def_entries(&name_refs[..15]), vec![]);
    let b = mod_with("b", "Bob", def_entries(&name_refs[..12]), vec![]);
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);

    let conflicts = likely_duplicate_mods(&scanned, &indices);

    assert_eq!(conflicts.len(), 1);
    let Conflict::LikelyDuplicateMod(c) = &conflicts[0] else {
        panic!("expected LikelyDuplicateMod")
    };
    assert_eq!(c.shared_defs, 12);
}

#[test]
fn likely_duplicate_not_flagged_when_a_relation_is_declared() {
    let names: Vec<String> = (0..15).map(|i| format!("Def{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let a = mod_with("a", "Alice", def_entries(&name_refs[..15]), vec![]);
    let b = mod_with_declared(
        "b",
        "Bob",
        def_entries(&name_refs[..12]),
        vec![],
        DeclaredOrder {
            load_after: vec![ModId::new("a")],
            ..DeclaredOrder::default()
        },
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);

    assert!(likely_duplicate_mods(&scanned, &indices).is_empty());
}

#[test]
fn likely_duplicate_not_flagged_below_the_minimum_shared_def_count() {
    let names: Vec<String> = (0..5).map(|i| format!("Def{i}")).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let a = mod_with("a", "Alice", def_entries(&name_refs[..5]), vec![]);
    let b = mod_with("b", "Bob", def_entries(&name_refs[..3]), vec![]);
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);

    assert!(likely_duplicate_mods(&scanned, &indices).is_empty());
}

fn template(name: &str) -> crate::domain::TemplateEntry {
    crate::domain::TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: "ThingDef".to_string(),
        name: name.to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }
}

#[test]
fn duplicate_template_name_requires_more_than_one_owner() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.templates = vec![template("WallBase")];
    let scanned = vec![a];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a")]);

    assert!(duplicate_template_names(&indices, &load_order).is_empty());
}

#[test]
fn duplicate_template_name_flags_two_mods_registering_the_same_name() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.templates = vec![template("WallBase")];
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.templates = vec![template("WallBase")];
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = duplicate_template_names(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::DuplicateTemplateName(c) = &conflicts[0] else {
        panic!("expected DuplicateTemplateName")
    };
    assert_eq!(c.name, "WallBase");
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("b")]);
}

#[test]
fn keyed_translation_collision_flags_two_mods_defining_the_same_key() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.translation_keys = std::collections::BTreeSet::from(["Greeting".to_string()]);
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.translation_keys = std::collections::BTreeSet::from(["Greeting".to_string()]);
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = keyed_translation_collisions(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::KeyedTranslationCollision(c) = &conflicts[0] else {
        panic!("expected KeyedTranslationCollision")
    };
    assert_eq!(c.key, "Greeting");
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("b")]);
}

#[test]
fn sound_override_flags_two_mods_shipping_the_same_normalized_path() {
    let mut a = mod_with("a", "Alice", vec![], vec![]);
    a.sounds = std::collections::BTreeSet::from(["shot_fire".to_string()]);
    let mut b = mod_with("b", "Bob", vec![], vec![]);
    b.sounds = std::collections::BTreeSet::from(["shot_fire".to_string()]);
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = sound_overrides(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::SoundOverride(c) = &conflicts[0] else {
        panic!("expected SoundOverride")
    };
    assert_eq!(c.path, "shot_fire");
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("b")]);
}

/// `assembly_name` distinguishes a genuinely independent patch DLL
/// from a bundled copy — see
/// `runtime_patch_collision_ignores_bundled_copies_of_the_same_assembly`.
fn runtime_target(
    assembly_name: &str,
    type_name: &str,
    method_name: &str,
) -> crate::domain::AssemblyInfo {
    crate::domain::AssemblyInfo {
        file_name: assembly_name.to_string(),
        name: assembly_name.to_string(),
        references: vec![],
        version: None,
        runtime_patches: vec![crate::domain::RuntimePatchTarget {
            type_name: type_name.to_string(),
            method_name: method_name.to_string(),
            kind: crate::domain::RuntimePatchKind::Unknown,
        }],
        type_hierarchy: Vec::new(),
        parse_failed: false,
    }
}

#[test]
fn runtime_patch_collision_flags_two_mods_patching_the_same_target() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target("a.dll", "Verse.Pawn", "Kill")],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target("b.dll", "Verse.Pawn", "Kill")],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = runtime_patch_collisions(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::RuntimePatchCollision(c) = &conflicts[0] else {
        panic!("expected RuntimePatchCollision")
    };
    assert_eq!(c.target_type, "Verse.Pawn");
    assert_eq!(c.target_method, "Kill");
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("b")]);
}

/// Two mods each bundling their own copy of the identically *named* assembly
/// aren't two independently-authored patches — just one patch DLL shipped
/// twice — so collapsing owners by originating assembly name before the `> 1`
/// test must suppress this collision even though there are two distinct
/// owning mods.
#[test]
fn runtime_patch_collision_ignores_bundled_copies_of_the_same_assembly() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target("shared.dll", "Verse.Pawn", "Kill")],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target("shared.dll", "Verse.Pawn", "Kill")],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    assert!(runtime_patch_collisions(&indices, &load_order).is_empty());
}

/// A third mod shipping a genuinely distinct assembly still makes it
/// a real collision, even though two of the three owners are bundled
/// copies of each other.
#[test]
fn runtime_patch_collision_flags_when_a_distinct_assembly_joins_bundled_copies() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target("shared.dll", "Verse.Pawn", "Kill")],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target("shared.dll", "Verse.Pawn", "Kill")],
    );
    let c = mod_with(
        "c",
        "Carol",
        vec![],
        vec![runtime_target("c.dll", "Verse.Pawn", "Kill")],
    );
    let scanned = vec![a, b, c];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]);

    let conflicts = runtime_patch_collisions(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::RuntimePatchCollision(c) = &conflicts[0] else {
        panic!("expected RuntimePatchCollision")
    };
    assert_eq!(
        c.owners,
        vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]
    );
}

#[test]
fn runtime_patch_collision_requires_more_than_one_owner() {
    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target("a.dll", "Verse.Pawn", "Kill")],
    );
    let scanned = vec![a];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a")]);

    assert!(runtime_patch_collisions(&indices, &load_order).is_empty());
}

// -- transpiler_collisions --------------------------------------------

/// [`runtime_target`]'s own sibling, for a target declared with a
/// specific [`crate::domain::RuntimePatchKind`] rather than always
/// `Unknown`.
fn runtime_target_kind(
    assembly_name: &str,
    type_name: &str,
    method_name: &str,
    kind: crate::domain::RuntimePatchKind,
) -> crate::domain::AssemblyInfo {
    crate::domain::AssemblyInfo {
        file_name: assembly_name.to_string(),
        name: assembly_name.to_string(),
        references: vec![],
        version: None,
        runtime_patches: vec![crate::domain::RuntimePatchTarget {
            type_name: type_name.to_string(),
            method_name: method_name.to_string(),
            kind,
        }],
        type_hierarchy: Vec::new(),
        parse_failed: false,
    }
}

#[test]
fn transpiler_collision_flags_two_mods_each_declaring_a_transpiler_on_the_same_target() {
    use crate::domain::RuntimePatchKind;

    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target_kind(
            "a.dll",
            "Verse.Verb_LaunchProjectile",
            "TryCastShot",
            RuntimePatchKind::Transpiler,
        )],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target_kind(
            "b.dll",
            "Verse.Verb_LaunchProjectile",
            "TryCastShot",
            RuntimePatchKind::Transpiler,
        )],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    let conflicts = transpiler_collisions(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::TranspilerCollision(c) = &conflicts[0] else {
        panic!("expected TranspilerCollision")
    };
    assert_eq!(c.target_type, "Verse.Verb_LaunchProjectile");
    assert_eq!(c.target_method, "TryCastShot");
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("b")]);
}

/// Two mods patching the same target with only `Prefix`/`Postfix` roles
/// produce an ordinary `RuntimePatchCollision` but never a
/// `TranspilerCollision` — prefixes/postfixes compose safely regardless of
/// load order, so only a genuine transpiler-vs-transpiler pair is
/// order-sensitive.
#[test]
fn transpiler_collision_never_fires_for_prefix_or_postfix_only_patchers() {
    use crate::domain::RuntimePatchKind;

    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target_kind(
            "a.dll",
            "Verse.Pawn",
            "Kill",
            RuntimePatchKind::Prefix,
        )],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target_kind(
            "b.dll",
            "Verse.Pawn",
            "Kill",
            RuntimePatchKind::Postfix,
        )],
    );
    let scanned = vec![a, b];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

    assert_eq!(runtime_patch_collisions(&indices, &load_order).len(), 1);
    assert!(transpiler_collisions(&indices, &load_order).is_empty());
}

/// A target patched by three mods — two `Transpiler`s and one
/// `Postfix`-only — must scope `owners` to the two transpiler
/// declarers alone, not every `RuntimePatchCollision` owner.
#[test]
fn transpiler_collision_scopes_owners_to_transpiler_declarers_only() {
    use crate::domain::RuntimePatchKind;

    let a = mod_with(
        "a",
        "Alice",
        vec![],
        vec![runtime_target_kind(
            "a.dll",
            "Verse.Verb_LaunchProjectile",
            "TryCastShot",
            RuntimePatchKind::Transpiler,
        )],
    );
    let b = mod_with(
        "b",
        "Bob",
        vec![],
        vec![runtime_target_kind(
            "b.dll",
            "Verse.Verb_LaunchProjectile",
            "TryCastShot",
            RuntimePatchKind::Postfix,
        )],
    );
    let c = mod_with(
        "c",
        "Carol",
        vec![],
        vec![runtime_target_kind(
            "c.dll",
            "Verse.Verb_LaunchProjectile",
            "TryCastShot",
            RuntimePatchKind::Transpiler,
        )],
    );
    let scanned = vec![a, b, c];
    let indices = build_indices(&scanned);
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]);

    let conflicts = transpiler_collisions(&indices, &load_order);

    assert_eq!(conflicts.len(), 1);
    let Conflict::TranspilerCollision(c) = &conflicts[0] else {
        panic!("expected TranspilerCollision")
    };
    assert_eq!(c.owners, vec![ModId::new("a"), ModId::new("c")]);
}

// -- missing_texture_paths --------------------------------------------

fn texture_candidate(
    def_type: &str,
    def_name: &str,
    field: &str,
    path: &str,
) -> TexturePathCandidate {
    TexturePathCandidate {
        container_tag: None,
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        field: field.to_string(),
        path: path.to_string(),
        graphic_class: None,
    }
}

/// Every `MissingTexturePath` row's path, in the order reported.
fn conflict_paths(conflicts: &[Conflict]) -> Vec<&str> {
    conflicts
        .iter()
        .map(|conflict| {
            let Conflict::MissingTexturePath(c) = conflict else {
                panic!("expected MissingTexturePath, got {conflict:?}")
            };
            c.path.as_str()
        })
        .collect()
}

/// The same candidate, declared as a `Graphic_Random` — i.e. one
/// `Verse.Graphic_Collection.Init` actually resolves, which is the
/// only case the strict direct-child folder rule applies to.
fn collection_texture_candidate(
    def_type: &str,
    def_name: &str,
    field: &str,
    path: &str,
) -> TexturePathCandidate {
    TexturePathCandidate {
        container_tag: None,
        graphic_class: Some("Graphic_Random".to_string()),
        ..texture_candidate(def_type, def_name, field, path)
    }
}

fn mod_with_texture_candidate(
    id: &str,
    may_require: Vec<String>,
    candidate: TexturePathCandidate,
) -> ScannedMod {
    let mut def = def_entry("ThingDef", "Referrer");
    def.may_require = may_require;
    ScannedMod {
        texture_path_candidates: vec![candidate],
        ..mod_with(id, "Alice", vec![def], vec![])
    }
}

#[test]
fn missing_texture_path_resolves_against_an_exact_shipped_path() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate(
            "ThingDef",
            "Referrer",
            "texPath",
            "Things/Item/Gun_Revolver",
        ),
    );
    referrer
        .textures
        .insert("things/item/gun_revolver".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// RimWorld resolves a base `texPath` against a suffixed family of
/// files for `Graphic_Multi`/rotation-aware graphics — the base path
/// itself is never shipped in that case.
#[test]
fn missing_texture_path_resolves_against_a_direction_suffixed_variant() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate(
            "ThingDef",
            "Referrer",
            "texPath",
            "Things/Item/Gun_Revolver",
        ),
    );
    referrer
        .textures
        .insert("things/item/gun_revolver_east".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// Same as the direction-suffix case, for a random-set (`_A`/`_B`/...)
/// multi-part variant.
#[test]
fn missing_texture_path_resolves_against_a_lettered_variant() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate(
            "ThingDef",
            "Referrer",
            "texPath",
            "Things/Item/Gun_Revolver",
        ),
    );
    referrer
        .textures
        .insert("things/item/gun_revolver_a".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// Real-install shape (`ExampleBiomes.Biomes`): a `texPath` naming a
/// directory whose single file happens to share the directory's own name
/// (`.../Aetheophyllum/Aetheophyllum.dds`) must resolve — a real, common
/// single-texture-per-subfolder authoring convention, distinct from the
/// rotation/random-set suffix case above.
#[test]
fn missing_texture_path_resolves_against_a_same_named_file_inside_a_matching_folder() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Aetheophyllum"),
    );
    referrer
        .textures
        .insert("plant/aetheophyllum/aetheophyllum".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// A collection folder holding *only* subfolder files still initializes — the
/// enumeration is recursive, so `Init` never logs `Collection cannot init` —
/// and then fails once per flattened path. One row per attempted path,
/// matching the game log line for line (reporting the folder once would not
/// match what the engine actually logs).
#[test]
fn missing_texture_path_reports_every_flattened_path_of_a_subfolder_only_folder() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate(
            "ThingDef",
            "Referrer",
            "texPath",
            "ExampleBiomes/Plant/Thornbrake",
        ),
    );
    for key in [
        "examplebiomes/plant/thornbrake/thornbrakegrown/thornbrakegrown_a",
        "examplebiomes/plant/thornbrake/thornbrakeimmature/thornbrakeimmature_a",
    ] {
        referrer.textures.insert(key.to_string(), 0);
    }
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(
        conflict_paths(&conflicts),
        vec![
            "examplebiomes/plant/thornbrake/thornbrakegrown_a",
            "examplebiomes/plant/thornbrake/thornbrakeimmature_a"
        ]
    );
}

/// A collection folder this index sees as empty — here because its
/// only entry is a `_m` mask, which `Init` filters out — falls back
/// to the ordinary check rather than being reported. **Core and the
/// DLCs ship no loose textures at all**, so every vanilla-provided
/// collection folder looks empty here; reporting emptiness produced
/// two real-install false positives against vanilla's own
/// `Things/Building/Natural/Hive`, and the game log has no
/// `Collection cannot init` line at all. Here the mask key still
/// makes the *folder* prefix match, so the lenient check resolves it.
#[test]
fn missing_texture_path_falls_back_when_a_collection_folder_looks_empty() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Masked"),
    );
    referrer
        .textures
        .insert("plant/masked/masked_m".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    assert!(missing_texture_paths(&scanned, &indices, &active_of(&scanned)).is_empty());
}

/// The same fallback with nothing shipped anywhere: the ordinary
/// check reports the folder path once, exactly as it did before the
/// collection model existed.
#[test]
fn missing_texture_path_still_reports_a_collection_path_nothing_ships() {
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Absent"),
    );
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(conflict_paths(&conflicts), vec!["plant/absent"]);
}

/// `GetAllUnderPath` only ever scans the `"{path}/"` prefix, so for a
/// collection graphic an exact file at the path itself — or a `path_A`-style
/// sibling — is not part of the enumeration at all and cannot rescue it. Both
/// short-circuit as "resolves" for every *non*-collection field, which the
/// next test pins.
#[test]
fn missing_texture_path_does_not_let_an_exact_file_rescue_a_collection_graphic() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Lone"),
    );
    // The exact path and a `_A` sibling both ship, and neither is
    // part of what `GetAllUnderPath("plant/lone/")` enumerates — only
    // the subfolder file is, and it flattens to a path nothing ships.
    referrer.textures.insert("plant/lone".to_string(), 0);
    referrer.textures.insert("plant/lone_a".to_string(), 0);
    referrer
        .textures
        .insert("plant/lone/sub/deep_a".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(conflict_paths(&conflicts), vec!["plant/lone/deep_a"]);
}

/// The same shipped files under a non-collection field resolve
/// exactly as they always did — the exact match wins.
#[test]
fn missing_texture_path_keeps_the_exact_match_for_a_non_collection_field() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Lone"),
    );
    referrer.textures.insert("plant/lone".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    assert!(missing_texture_paths(&scanned, &indices, &active_of(&scanned)).is_empty());
}

/// The dominant real authoring shape declares `graphicClass` once on a
/// `ParentName` template and only `texPath` on each concrete def — 30 vanilla
/// templates and 574 inheriting defs on the real install. `XmlInheritance`
/// merges the template's `<graphicData>` into the child's, so the strict
/// collection rule has to walk the chain.
#[test]
fn missing_texture_path_resolves_the_graphic_class_through_the_parent_name_chain() {
    let mut base = mod_with("base", "Alice", vec![], vec![]);
    base.templates = vec![crate::domain::TemplateEntry {
        def_type: "ThingDef".to_string(),
        name: "PlantBase".to_string(),
        parent_name: None,
        may_require: Vec::new(),
        graphic_class: Some("Graphic_Random".to_string()),
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];

    let mut inheriting_def = def_entry("ThingDef", "Referrer");
    inheriting_def.parent_name = Some("PlantBase".to_string());
    let referrer = ScannedMod {
        texture_path_candidates: vec![TexturePathCandidate {
            def_type: "ThingDef".to_string(),
            def_name: "Referrer".to_string(),
            field: "texPath".to_string(),
            path: "Plant/Lone".to_string(),
            graphic_class: None,
            container_tag: Some("graphicData".to_string()),
        }],
        textures: [
            ("plant/lone".to_string(), 0),
            ("plant/lone/sub/deep_a".to_string(), 0),
        ]
        .into_iter()
        .collect(),
        ..mod_with("referrer", "Alice", vec![inheriting_def], vec![])
    };
    let scanned = vec![base, referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(
        conflict_paths(&conflicts),
        vec!["plant/lone/deep_a"],
        "the inherited Graphic_Random is what makes the subfolder file a failure"
    );
}

/// The chain walk must not reach a `texPath` that has nothing to do
/// with the def's graphic — a comp's own path, or one a mod's own def
/// type reads. Same fixture as above, only the container differs.
#[test]
fn missing_texture_path_does_not_inherit_a_graphic_class_outside_graphic_data() {
    let mut base = mod_with("base", "Alice", vec![], vec![]);
    base.templates = vec![crate::domain::TemplateEntry {
        def_type: "ThingDef".to_string(),
        name: "PlantBase".to_string(),
        parent_name: None,
        may_require: Vec::new(),
        graphic_class: Some("Graphic_Random".to_string()),
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];

    let mut inheriting_def = def_entry("ThingDef", "Referrer");
    inheriting_def.parent_name = Some("PlantBase".to_string());
    let referrer = ScannedMod {
        texture_path_candidates: vec![TexturePathCandidate {
            def_type: "ThingDef".to_string(),
            def_name: "Referrer".to_string(),
            field: "texPath".to_string(),
            path: "Plant/Lone".to_string(),
            graphic_class: None,
            container_tag: Some("li".to_string()),
        }],
        textures: [
            ("plant/lone".to_string(), 0),
            ("plant/lone/sub/deep_a".to_string(), 0),
        ]
        .into_iter()
        .collect(),
        ..mod_with("referrer", "Alice", vec![inheriting_def], vec![])
    };
    let scanned = vec![base, referrer];
    let indices = build_indices(&scanned);

    assert!(missing_texture_paths(&scanned, &indices, &active_of(&scanned)).is_empty());
}

/// The counterpart of the test above: one file directly in the folder
/// is all `Graphic_Collection.Init` needs to initialize.
#[test]
fn missing_texture_path_resolves_a_folder_with_a_direct_child() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Sigillaria"),
    );
    referrer
        .textures
        .insert("plant/sigillaria/sigillaria_a".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// The real `Sigillaria` shape: the folder *does* hold direct files, so
/// it initializes — but `Init` also enumerates the subfolders recursively and
/// then looks each of those files up at folder-plus-file-name, which does not
/// exist. One row per attempted path, exactly as the game logs it.
#[test]
fn missing_texture_path_reports_the_flattened_path_of_a_subfolder_texture() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Sigillaria"),
    );
    for key in [
        "plant/sigillaria/sigillaria_a",
        "plant/sigillaria/sigillariagrown/sigillariagrown_a",
        "plant/sigillaria/sigillariagrown/sigillariagrown_b",
    ] {
        referrer.textures.insert(key.to_string(), 0);
    }
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(
        conflict_paths(&conflicts),
        vec![
            "plant/sigillaria/sigillariagrown_a",
            "plant/sigillaria/sigillariagrown_b"
        ]
    );
}

/// The four `ExampleBiomes` defs that ship `Dry/` variants and log
/// nothing: the subfolder file's own name matches a direct sibling,
/// so the flattened lookup finds that sibling and succeeds. This is
/// why the check asks whether the flattened path exists rather than
/// merely whether a key is nested.
#[test]
fn missing_texture_path_ignores_a_subfolder_texture_shadowed_by_a_direct_sibling() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        collection_texture_candidate("ThingDef", "Referrer", "texPath", "Plant/Crinoid"),
    );
    for key in ["plant/crinoid/crinoid_a", "plant/crinoid/dry/crinoid_a"] {
        referrer.textures.insert(key.to_string(), 0);
    }
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

/// Real-install shape: the strict direct-child rule must **not** reach a
/// `texPath` no `Graphic_Collection` ever resolves. Example Animation's
/// `BrowTypeDef`/`HeadTypeDef`/`LidTypeDef` paths (227 of them on the real
/// install) name a folder holding only `Male/` and `Female/` subfolders, its
/// own C# reads them, and the game logs nothing — flagging those would be 227
/// false positives against zero true ones.
#[test]
fn missing_texture_path_keeps_the_lenient_folder_rule_without_a_collection_graphic_class() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate(
            "ExampleAnim.BrowTypeDef",
            "Referrer",
            "texPath",
            "Brows/Angry",
        ),
    );
    referrer
        .textures
        .insert("brows/angry/female/angled_east".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

#[test]
fn missing_texture_path_reports_a_path_no_active_mod_ships() {
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Missing"),
    );
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(conflicts.len(), 1);
    let Conflict::MissingTexturePath(c) = &conflicts[0] else {
        panic!("expected MissingTexturePath")
    };
    assert_eq!(c.referrer, ModId::new("referrer"));
    assert_eq!(c.field, "texPath");
    assert_eq!(c.path, "things/item/gun_missing");
}

/// A near-miss: a different, longer shipped path that merely starts
/// with the same *characters* as the candidate but has no `_`
/// separator right after it (`"gunner"`, not `"gun_ner"`) must not
/// read as a variant match — only a genuine `path_<suffix>` shape
/// counts.
#[test]
fn missing_texture_path_does_not_treat_an_unrelated_longer_path_as_a_variant() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun"),
    );
    referrer
        .textures
        .insert("things/item/gunner".to_string(), 0);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(
        conflicts.len(),
        1,
        "\"gunner\" must not satisfy \"gun\"'s own variant check"
    );
}

/// Real-install finding: vanilla's own procedurally generated content (book
/// covers, gene/`DrawStyleDef` icons) has no literal file behind its
/// `texPath` at all — a Core/DLC referrer is excluded outright rather than
/// trying to classify which vanilla gaps are "real".
#[test]
fn missing_texture_path_excludes_a_vanilla_referrer() {
    let mut referrer = mod_with_texture_candidate(
        "core",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Missing"),
    );
    referrer.info.source = Source::Core;
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

#[test]
fn missing_texture_path_skips_a_candidate_on_a_may_require_gated_def() {
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec!["not.active".to_string()],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Missing"),
    );
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

// -- The non-loose (asset bundle / Core resource) index -----------------

#[test]
fn a_texpath_served_by_core_resources_is_not_missing() {
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Core"),
    );
    let scanned = vec![referrer];
    let mut core_resources = large_core_resource_index();
    core_resources.insert("things/item/gun_core".to_string());
    let indices = Indices::build(
        &scanned,
        &HashSet::new(),
        &active_of(&scanned),
        &core_resources,
    );

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

#[test]
fn a_texpath_served_by_an_active_dlc_bundle_is_not_missing() {
    let mut referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Dlc"),
    );
    let mut dlc = mod_with("biotech", "Ludeon Studios", vec![], vec![]);
    dlc.info.source = Source::Dlc;
    dlc.bundle_textures
        .insert("things/item/gun_dlc".to_string());
    referrer.info.source = Source::Local;
    let scanned = vec![referrer, dlc];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(conflicts.is_empty());
}

#[test]
fn a_texpath_in_an_inactive_dlc_bundle_still_flags() {
    // No mod in `scanned` ships `things/item/gun_missing` in either
    // `textures` or `bundle_textures` — an inactive DLC never becomes a
    // `ScannedMod` at all, so this is exactly what that shape looks like
    // here.
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Missing"),
    );
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(conflicts.len(), 1);
}

#[test]
fn a_texpath_with_a_format_placeholder_is_skipped() {
    let brace = mod_with_texture_candidate(
        "brace",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/{0}"),
    );
    let bracket = mod_with_texture_candidate(
        "bracket",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/[Colour]"),
    );
    let scanned = vec![brace, bracket];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(
        conflicts.is_empty(),
        "a value naming a format string or encoded data is never a candidate"
    );
}

#[test]
fn a_collection_folder_empty_everywhere_now_reports_a_row() {
    let mut candidate = texture_candidate(
        "ThingDef",
        "Referrer",
        "texPath",
        "Things/Building/Natural/Hive",
    );
    candidate.graphic_class = Some("Graphic_Random".to_string());
    candidate.container_tag = Some("graphicData".to_string());
    let referrer = mod_with_texture_candidate("referrer", vec![], candidate);
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert_eq!(
        conflicts.len(),
        1,
        "empty in every source (loose, bundle, and core) is a real gap"
    );
}

#[test]
fn a_collection_folder_served_only_by_a_bundle_resolves_with_flattened_names() {
    let mut candidate = texture_candidate(
        "ThingDef",
        "Referrer",
        "texPath",
        "Things/Building/Natural/Hive",
    );
    candidate.graphic_class = Some("Graphic_Random".to_string());
    candidate.container_tag = Some("graphicData".to_string());
    let mut referrer = mod_with_texture_candidate("referrer", vec![], candidate);
    referrer
        .bundle_textures
        .insert("things/building/natural/hive/hive_a".to_string());
    let scanned = vec![referrer];
    let indices = build_indices(&scanned);

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(
        conflicts.is_empty(),
        "the bundle-served flattened name resolves the collection graphic"
    );
}

#[test]
fn an_unreadable_core_index_skips_the_check_with_a_warning() {
    let referrer = mod_with_texture_candidate(
        "referrer",
        vec![],
        texture_candidate("ThingDef", "Referrer", "texPath", "Things/Item/Gun_Missing"),
    );
    let scanned = vec![referrer];
    // Below `MIN_CORE_RESOURCE_TEXTURES` — the fail-safe, not
    // `build_indices`' own padded default.
    let indices = Indices::build(
        &scanned,
        &HashSet::new(),
        &active_of(&scanned),
        &BTreeSet::new(),
    );

    let conflicts = missing_texture_paths(&scanned, &indices, &active_of(&scanned));

    assert!(
        conflicts.is_empty(),
        "a near-empty core index disables the check entirely rather than \
         running degraded"
    );
    assert!(
        crate::analysis::checks::core_resource_index_warning(indices.core_resource_texture_count)
            .is_some()
    );
}

// -- `PatchCollision::winner_declares_relation` ------------------------

fn loads_after(ids: &[&str]) -> DeclaredOrder {
    DeclaredOrder {
        load_after: ids.iter().map(|id| ModId::new(*id)).collect(),
        ..DeclaredOrder::default()
    }
}

/// One mod per `(id, op class, declared order)` entry, all patching
/// `ExampleDef/label` (or the def root when `field` is `None`), in the
/// order given as the load order. Returns the single collision found and
/// the scanned mods' report entries.
fn collision_of(
    patchers: Vec<(&str, &str, DeclaredOrder)>,
    field: Option<&str>,
) -> (crate::domain::PatchCollision, Vec<Mod>) {
    collision_with_lone_ops(patchers, field, Vec::new())
}

/// [`collision_of`] plus `lone_ops`: `(id, op class, field)` operations by
/// further mods on the same def, each alone on its own target so it adds
/// no collision of its own.
fn collision_with_lone_ops(
    patchers: Vec<(&str, &str, DeclaredOrder)>,
    field: Option<&str>,
    lone_ops: Vec<(&str, &str, Option<&str>)>,
) -> (crate::domain::PatchCollision, Vec<Mod>) {
    let target = DefTarget {
        sub_path: field.map(str::to_string),
        ..def_name_target("ThingDef", "ExampleDef")
    };
    let load_order = LoadOrder::new(patchers.iter().map(|(id, ..)| ModId::new(*id)).collect());
    let mut scanned: Vec<ScannedMod> = patchers
        .into_iter()
        .map(|(id, op_class, declared)| {
            let mut patcher = mod_with_declared(id, id, vec![], vec![], declared);
            patcher.patch_ops = vec![patch_op(op_class, target.clone())];
            patcher
        })
        .collect();
    for (id, op_class, lone_field) in lone_ops {
        let lone_target = DefTarget {
            sub_path: lone_field.map(str::to_string),
            ..def_name_target("ThingDef", "ExampleDef")
        };
        let mut lone = mod_with(id, id, vec![], vec![]);
        lone.patch_ops = vec![patch_op(op_class, lone_target)];
        scanned.push(lone);
    }

    let conflicts = patch_collisions(
        &scanned,
        &load_order,
        &active_of(&scanned),
        &DisplayNameIndex::new(),
    );

    let [Conflict::PatchCollision(collision)] = conflicts.as_slice() else {
        panic!("expected exactly one patch collision, got {conflicts:?}");
    };
    let infos = scanned.into_iter().map(|sm| sm.info).collect();
    (collision.clone(), infos)
}

/// Whether the collision's last contributor (the winner in the order the
/// patchers were given) deliberately ordered itself after the rest.
fn winner_declares(collision: &crate::domain::PatchCollision, mods: &[Mod]) -> bool {
    let by_id: ModsById<'_> = mods.iter().map(|m| (m.id.clone(), m)).collect();
    let winner = &collision.mods.last().expect("a collision has mods").mod_id;
    collision.winner_declares_relation(winner, &by_id)
}

#[test]
fn a_winner_declaring_load_after_the_other_patcher_is_flagged() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.framework",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                loads_after(&["example.framework"]),
            ),
        ],
        Some("label"),
    );

    assert!(winner_declares(&collision, &mods));
}

#[test]
fn a_winner_declaring_a_dependency_on_the_other_patcher_is_flagged() {
    let declared = DeclaredOrder {
        dependencies: vec![crate::domain::ModDependency {
            id: ModId::new("example.framework"),
            display_name: None,
        }],
        ..DeclaredOrder::default()
    };
    let (collision, mods) = collision_of(
        vec![
            (
                "example.framework",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            ("example.patcher", "PatchOperationReplace", declared),
        ],
        Some("label"),
    );

    assert!(winner_declares(&collision, &mods));
}

#[test]
fn a_winner_declaring_nothing_is_not_flagged() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.framework",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
        ],
        Some("label"),
    );

    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_declaration_toward_only_one_of_three_patchers_is_not_flagged() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.first",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            (
                "example.second",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                loads_after(&["example.second"]),
            ),
        ],
        Some("label"),
    );

    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_declared_patcher_that_removes_the_field_blocks_the_flag() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.remover",
                "PatchOperationRemove",
                DeclaredOrder::default(),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                loads_after(&["example.remover"]),
            ),
        ],
        Some("label"),
    );

    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_collision_on_the_def_root_is_not_flagged() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.framework",
                "PatchOperationReplace",
                DeclaredOrder::default(),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                loads_after(&["example.framework"]),
            ),
        ],
        None,
    );

    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_mutual_declaration_is_not_flagged() {
    let (collision, mods) = collision_of(
        vec![
            (
                "example.framework",
                "PatchOperationReplace",
                loads_after(&["example.patcher"]),
            ),
            (
                "example.patcher",
                "PatchOperationReplace",
                loads_after(&["example.framework"]),
            ),
        ],
        Some("label"),
    );

    assert!(!winner_declares(&collision, &mods));
}

fn declared_pair() -> Vec<(&'static str, &'static str, DeclaredOrder)> {
    vec![
        (
            "example.framework",
            "PatchOperationReplace",
            DeclaredOrder::default(),
        ),
        (
            "example.patcher",
            "PatchOperationReplace",
            loads_after(&["example.framework"]),
        ),
    ]
}

#[test]
fn a_remover_of_an_ancestor_field_is_recorded_and_blocks_the_flag() {
    let (collision, mods) = collision_with_lone_ops(
        declared_pair(),
        Some("comps/li/label"),
        vec![("example.remover", "PatchOperationRemove", Some("comps"))],
    );

    assert_eq!(collision.removed_by, vec![ModId::new("example.remover")]);
    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_whole_def_remover_is_recorded_and_blocks_the_flag() {
    let (collision, mods) = collision_with_lone_ops(
        declared_pair(),
        Some("label"),
        vec![("example.remover", "PatchOperationRemove", None)],
    );

    assert_eq!(collision.removed_by, vec![ModId::new("example.remover")]);
    assert!(!winner_declares(&collision, &mods));
}

#[test]
fn a_remover_of_a_sibling_field_or_a_longer_name_does_not_count() {
    let (collision, mods) = collision_with_lone_ops(
        declared_pair(),
        Some("comps"),
        vec![
            ("example.sibling", "PatchOperationRemove", Some("label")),
            ("example.prefix", "PatchOperationRemove", Some("comps2")),
            ("example.deeper", "PatchOperationRemove", Some("comps/li")),
        ],
    );

    assert!(collision.removed_by.is_empty());
    assert!(winner_declares(&collision, &mods));
}
