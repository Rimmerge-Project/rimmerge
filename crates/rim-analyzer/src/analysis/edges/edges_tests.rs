//! Tests for the edge builders.

use super::assembly::{SHARED_LIBRARY_OWNER_THRESHOLD, is_ignored_assembly};
use super::defs::def_name_family_prefix;
use super::patches::{
    InlineCheck, SeenEdges, emit_path_injected_node_edge, parse_simple_equality_predicate,
};
use super::*;
use crate::analysis::indices::{ActiveMods, DefKey, Indices};
use crate::analysis::inheritance::TemplateRegistrations;
use crate::domain::{
    ConditionalBranch, Conflict, DeclaredOrder, DefTarget, FindModGate, Mod, ModDependency,
    PatchOp, Source, ValueDigest, XmlLocator,
};
use crate::domain::{
    Constraint, ConstraintStatus, Edge, EdgeKind, EdgeStrength, LoadOrder, ModId, ScannedMod,
    Selector,
};
use std::collections::BTreeSet;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

fn active_of(scanned: &[ScannedMod]) -> ActiveMods {
    ActiveMods::build(scanned)
}

fn bare_mod(id: &str, name: &str) -> Mod {
    Mod {
        id: ModId::new(id),
        name: name.to_string(),
        authors: Vec::new(),
        url: None,
        path: PathBuf::from(id),
        source: Source::Local,
        supported_versions: Vec::new(),
        declared: DeclaredOrder::default(),
        loaded_folders: Vec::new(),
        hard_dependents: 0,
        soft_dependents: 0,
        awareness_dependents: 0,
        is_framework_candidate: false,
        generated: None,
        workshop_id: None,
        load_folders_version_matched: None,
    }
}

fn scanned(info: Mod, assemblies: Vec<crate::domain::AssemblyInfo>) -> ScannedMod {
    ScannedMod {
        info,
        defs: Vec::new(),
        templates: Vec::new(),
        patch_ops: Vec::new(),
        textures: BTreeMap::new(),
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

/// One shipped assembly named `name` referencing each of `references`
/// (name, load_time).
fn assembly(name: &str, references: Vec<(&str, bool)>) -> crate::domain::AssemblyInfo {
    crate::domain::AssemblyInfo {
        file_name: name.to_string(),
        name: name.to_string(),
        references: references
            .into_iter()
            .map(|(name, load_time)| crate::domain::AssemblyReference {
                name: name.to_string(),
                load_time,
            })
            .collect(),
        version: None,
        runtime_patches: Vec::new(),
        type_hierarchy: Vec::new(),
        parse_failed: false,
    }
}

fn patch_op(class: &str, target: Option<DefTarget>) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        class: class.to_string(),
        xpath: None,
        target,
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
fn assembly_ref_edge_points_from_referencer_to_owner() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("a.dll", vec![])]);
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("a.dll", true)])],
    );
    let scanned_mods = vec![a, b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );

    let edges = assembly_ref_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("b"));
    assert_eq!(edges[0].before, ModId::new("a"));
    assert_eq!(edges[0].kind, EdgeKind::AssemblyRef);
    assert!(edges[0].load_time);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Hard);
}

/// A reference resolved only through a method body/field/attribute
/// (never a `TypeDef.Extends`/`InterfaceImpl.Interface`) is lazy: it
/// downgrades the edge to `Soft`, not `Hard`.
#[test]
fn lazy_assembly_ref_edge_is_soft_strength() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("a.dll", vec![])]);
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("a.dll", false)])],
    );
    let scanned_mods = vec![a, b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );

    let edges = assembly_ref_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert!(!edges[0].load_time);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Soft);
}

/// An assembly name shipped by more than one active mod must not
/// produce any per-candidate edge at all — that ambiguity is modeled
/// as an any-of [`Constraint`] instead (see
/// [`assembly_ref_constraints`]).
#[test]
fn ambiguous_owner_produces_no_edges() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("shared.dll", vec![])]);
    let a2 = scanned(bare_mod("a2", "A2"), vec![assembly("shared.dll", vec![])]);
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("shared.dll", true)])],
    );
    let scanned_mods = vec![a, a2, b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );

    assert!(assembly_ref_edges(&scanned_mods, &indices).is_empty());
}

#[test]
fn ambiguous_owner_constraint_satisfied_when_a_candidate_loads_first() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("shared.dll", vec![])]);
    let a2 = scanned(bare_mod("a2", "A2"), vec![assembly("shared.dll", vec![])]);
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("shared.dll", true)])],
    );
    let scanned_mods = vec![a, a2, b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("a2"), ModId::new("b")]);

    let constraints = assembly_ref_constraints(&scanned_mods, &indices, &load_order);

    assert_eq!(constraints.len(), 1);
    let Constraint::AnyOf {
        after,
        assembly,
        candidates,
        load_time,
        status,
    } = &constraints[0];
    assert_eq!(*after, ModId::new("b"));
    assert_eq!(assembly.as_str(), "shared.dll");
    assert_eq!(candidates.len(), 2);
    assert!(*load_time);
    assert_eq!(*status, ConstraintStatus::Satisfied);
}

#[test]
fn ambiguous_owner_constraint_violated_when_no_candidate_loads_first() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("shared.dll", vec![])]);
    let a2 = scanned(bare_mod("a2", "A2"), vec![assembly("shared.dll", vec![])]);
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("shared.dll", true)])],
    );
    let scanned_mods = vec![a, a2, b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let load_order = LoadOrder::new(vec![ModId::new("b"), ModId::new("a"), ModId::new("a2")]);

    let constraints = assembly_ref_constraints(&scanned_mods, &indices, &load_order);

    assert_eq!(constraints.len(), 1);
    let Constraint::AnyOf { status, .. } = &constraints[0];
    assert_eq!(*status, ConstraintStatus::Violated);
}

/// `count` mods, each shipping its own copy of a DLL named `name` and
/// nothing else — the shape [`is_shared_library`]'s owner-count inference
/// reads off [`Indices::assembly_owners`].
fn mods_shipping(name: &str, count: usize) -> Vec<ScannedMod> {
    (0..count)
        .map(|i| {
            let id = format!("shipper{i}");
            scanned(bare_mod(&id, &id), vec![assembly(name, vec![])])
        })
        .collect()
}

/// A name shipped by [`SHARED_LIBRARY_OWNER_THRESHOLD`] distinct mods is
/// inferred as a shared library: a referencing mod that doesn't ship its
/// own copy gets neither a per-candidate edge (ambiguous anyway, since
/// there are several owners) nor an any-of constraint.
#[test]
fn a_shared_library_inferred_from_owner_count_produces_no_edge_or_constraint() {
    let mut scanned_mods = mods_shipping("sharedlib", SHARED_LIBRARY_OWNER_THRESHOLD);
    scanned_mods.push(scanned(
        bare_mod("consumer", "Consumer"),
        vec![assembly("consumer.dll", vec![("sharedlib", true)])],
    ));
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let load_order = LoadOrder::new(scanned_mods.iter().map(|m| m.info.id.clone()).collect());

    assert!(assembly_ref_edges(&scanned_mods, &indices).is_empty());
    assert!(assembly_ref_constraints(&scanned_mods, &indices, &load_order).is_empty());
}

/// One owner short of [`SHARED_LIBRARY_OWNER_THRESHOLD`], the same shape
/// still produces a real any-of constraint — the inference must not fire
/// early. This is the boundary [`a_shared_library_inferred_from_owner_count_produces_no_edge_or_constraint`]
/// checks from the other side.
#[test]
fn one_owner_short_of_the_shared_library_threshold_still_yields_a_constraint() {
    let below_threshold = SHARED_LIBRARY_OWNER_THRESHOLD - 1;
    let mut scanned_mods = mods_shipping("almostshared", below_threshold);
    scanned_mods.push(scanned(
        bare_mod("consumer", "Consumer"),
        vec![assembly("consumer.dll", vec![("almostshared", true)])],
    ));
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let load_order = LoadOrder::new(scanned_mods.iter().map(|m| m.info.id.clone()).collect());

    let constraints = assembly_ref_constraints(&scanned_mods, &indices, &load_order);
    assert_eq!(constraints.len(), 1);
    let Constraint::AnyOf { candidates, .. } = &constraints[0];
    assert_eq!(candidates.len(), below_threshold);
}

/// A genuine two-mod provider/consumer pair — well under the shared-library
/// threshold — must still yield its own edge, never swallowed by the
/// inference.
#[test]
fn a_two_mod_provider_consumer_pair_still_yields_its_edge() {
    let provider = scanned(
        bare_mod("provider", "Provider"),
        vec![assembly("providerlib", vec![])],
    );
    let consumer = scanned(
        bare_mod("consumer", "Consumer"),
        vec![assembly("consumer.dll", vec![("providerlib", true)])],
    );
    let scanned_mods = vec![provider, consumer];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );

    let edges = assembly_ref_edges(&scanned_mods, &indices);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("provider"));
}

#[test]
fn system_dot_prefixed_assemblies_are_ignored_but_systemx_is_not() {
    let owners = BTreeMap::new();
    assert!(is_ignored_assembly("system", &HashSet::new(), &owners));
    assert!(is_ignored_assembly("system.core", &HashSet::new(), &owners));
    assert!(!is_ignored_assembly("systemx", &HashSet::new(), &owners));
}

#[test]
fn self_shipped_reference_produces_no_edge() {
    let b = scanned(
        bare_mod("b", "B"),
        vec![assembly("b.dll", vec![("b.dll", true)])],
    );
    let scanned_mods = vec![b];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    assert!(assembly_ref_edges(&scanned_mods, &indices).is_empty());
}

#[test]
fn mod_dependency_implies_load_after() {
    let mut info = bare_mod("b", "B");
    info.declared.dependencies = vec![ModDependency {
        id: ModId::new("a"),
        display_name: None,
    }];
    let scanned_mods = vec![scanned(bare_mod("a", "A"), vec![]), scanned(info, vec![])];
    let active = active_of(&scanned_mods);

    let edges = declared_edges(&scanned_mods, &active);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("b"));
    assert_eq!(edges[0].before, ModId::new("a"));
    assert_eq!(edges[0].kind, EdgeKind::ModDependency);
}

#[test]
fn load_before_flips_direction() {
    let mut info = bare_mod("b", "B");
    info.declared.load_before = vec![ModId::new("a")];
    let scanned_mods = vec![scanned(bare_mod("a", "A"), vec![]), scanned(info, vec![])];
    let active = active_of(&scanned_mods);

    let edges = declared_edges(&scanned_mods, &active);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("a"));
    assert_eq!(edges[0].before, ModId::new("b"));
}

#[test]
fn declared_edge_skipped_when_target_is_inactive() {
    let mut info = bare_mod("b", "B");
    info.declared.load_after = vec![ModId::new("inactive")];
    let scanned_mods = vec![scanned(info, vec![])];
    let active = active_of(&scanned_mods);

    assert!(declared_edges(&scanned_mods, &active).is_empty());
}

/// Regression guard: `About.xml` targets never carry the `_steam`
/// suffix RimWorld may append in `ModsConfig.xml`, so the edge must
/// still point at the exact active id — matching what `LoadOrder`
/// and the target's own `Mod::id` use — not the bare declared one.
#[test]
fn declared_edge_resolves_a_bare_target_to_its_steam_suffixed_active_id() {
    let mut info = bare_mod("b", "B");
    info.declared.load_after = vec![ModId::new("a")];
    let scanned_mods = vec![
        scanned(bare_mod("a_steam", "A"), vec![]),
        scanned(info, vec![]),
    ];
    let active = active_of(&scanned_mods);

    let edges = declared_edges(&scanned_mods, &active);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("b"));
    assert_eq!(edges[0].before, ModId::new("a_steam"));
}

#[test]
fn find_mod_resolves_display_name_case_insensitively() {
    let info_b = bare_mod("b", "B Mod");
    let mut b = scanned(info_b, vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_names: vec!["a mod".into()],
        ..patch_op("PatchOperationFindMod", None)
    }];
    let a = scanned(bare_mod("a", "A Mod"), vec![]);
    let scanned_mods = vec![a, b];
    let (name_map, warnings) = build_name_map(&scanned_mods);
    assert!(warnings.is_empty());
    let active = active_of(&scanned_mods);

    let result = find_mod_edges(&scanned_mods, &name_map, &active);

    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].before, ModId::new("a"));
    assert!(result.unresolved.is_empty());
    assert!(result.using_package_id.is_empty());
}

#[test]
fn find_mod_unresolved_name_is_reported_not_dropped() {
    let mut b = scanned(bare_mod("b", "B Mod"), vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_names: vec!["Nonexistent Mod".into()],
        ..patch_op("PatchOperationFindMod", None)
    }];
    let scanned_mods = vec![b];
    let (name_map, _) = build_name_map(&scanned_mods);
    let active = active_of(&scanned_mods);

    let result = find_mod_edges(&scanned_mods, &name_map, &active);

    assert!(result.edges.is_empty());
    assert_eq!(result.unresolved.len(), 1);
    assert_eq!(result.unresolved[0].display_name, "Nonexistent Mod");
    assert!(result.using_package_id.is_empty());
}

#[test]
fn find_mod_name_matching_an_active_package_id_is_reported_separately() {
    let mut b = scanned(bare_mod("b", "B Mod"), vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_names: vec!["author.othermod".into()],
        ..patch_op("PatchOperationFindMod", None)
    }];
    let other = scanned(bare_mod("author.othermod", "Something Else"), vec![]);
    let scanned_mods = vec![b, other];
    let (name_map, _) = build_name_map(&scanned_mods);
    let active = active_of(&scanned_mods);

    let result = find_mod_edges(&scanned_mods, &name_map, &active);

    assert!(result.edges.is_empty());
    assert!(result.unresolved.is_empty());
    assert_eq!(result.using_package_id.len(), 1);
    assert_eq!(result.using_package_id[0].display_name, "author.othermod");
}

/// A `PatchOperationFindMod` with only a `<nomatch>` branch (or no
/// branch at all) still yields the awareness edge implied by its own
/// `<mods>` list — guards against `find_mod_edges` only ever reading
/// descendants gated by a `<match>` branch.
#[test]
fn nomatch_only_find_mod_still_yields_awareness_edge() {
    let mut b = scanned(bare_mod("b", "B Mod"), vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_names: vec!["A Mod".into()],
        ..patch_op("PatchOperationFindMod", None)
    }];
    let a = scanned(bare_mod("a", "A Mod"), vec![]);
    let scanned_mods = vec![a, b];
    let (name_map, _) = build_name_map(&scanned_mods);
    let active = active_of(&scanned_mods);

    let result = find_mod_edges(&scanned_mods, &name_map, &active);

    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].before, ModId::new("a"));
    assert_eq!(result.edges[0].kind, EdgeKind::FindMod);
}

#[test]
fn build_name_map_warns_on_duplicate_display_name() {
    let a = scanned(bare_mod("a", "Same Name"), vec![]);
    let b = scanned(bare_mod("b", "Same Name"), vec![]);
    let scanned_mods = vec![a, b];

    let (name_map, warnings) = build_name_map(&scanned_mods);

    assert_eq!(name_map.get("same name"), Some(&ModId::new("a")));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn patch_target_edge_requires_exactly_one_non_vanilla_owner() {
    use crate::domain::DefEntry;
    let mut owner = scanned(bare_mod("owner", "Owner"), vec![]);
    owner.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "Wall".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        Some(def_name_target("ThingDef", "Wall")),
    )];
    let scanned_mods = vec![owner, patcher];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_target_edges(
        &scanned_mods,
        &indices,
        &active,
        &name_map,
        &BTreeMap::new(),
    );

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("patcher"));
    assert_eq!(edges[0].before, ModId::new("owner"));
    assert_eq!(edges[0].kind, EdgeKind::PatchTargetsDef);
}

#[test]
fn patch_target_edge_skipped_when_def_owned_by_vanilla() {
    use crate::domain::DefEntry;
    let mut core = scanned(bare_mod("core", "Core"), vec![]);
    core.info.source = Source::Core;
    core.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "Wall".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        Some(def_name_target("ThingDef", "Wall")),
    )];
    let scanned_mods = vec![core, patcher];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_target_edges(
            &scanned_mods,
            &indices,
            &active,
            &name_map,
            &BTreeMap::new()
        )
        .is_empty()
    );
}

#[test]
fn patch_target_edge_skipped_when_target_selector_is_name_attribute() {
    use crate::domain::DefEntry;
    let mut owner = scanned(bare_mod("owner", "Owner"), vec![]);
    owner.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "WallBase".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        Some(DefTarget {
            def_type: "ThingDef".into(),
            def_name: "WallBase".into(),
            selector: Selector::NameAttr,
            sub_path: None,
        }),
    )];
    let scanned_mods = vec![owner, patcher];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_target_edges(
            &scanned_mods,
            &indices,
            &active,
            &name_map,
            &BTreeMap::new()
        )
        .is_empty()
    );
}

#[test]
fn patch_target_edge_skipped_when_may_require_is_unsatisfied() {
    use crate::domain::DefEntry;
    let mut owner = scanned(bare_mod("owner", "Owner"), vec![]);
    owner.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "Wall".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        may_require: vec!["not.active".to_string()],
        // A list item is the one shape the game actually reads MayRequire
        // on; a top-level op's own attribute is unread.
        is_list_item: true,
        ..patch_op(
            "PatchOperationReplace",
            Some(def_name_target("ThingDef", "Wall")),
        )
    }];
    let scanned_mods = vec![owner, patcher];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_target_edges(
            &scanned_mods,
            &indices,
            &active,
            &name_map,
            &BTreeMap::new()
        )
        .is_empty()
    );
}

#[test]
fn patch_target_edge_skipped_when_enclosing_find_mod_is_inactive() {
    use crate::domain::DefEntry;
    let mut owner = scanned(bare_mod("owner", "Owner"), vec![]);
    owner.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "Wall".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_context: vec![FindModGate::AnyActive(vec!["Nonexistent Mod".to_string()])],
        ..patch_op(
            "PatchOperationReplace",
            Some(def_name_target("ThingDef", "Wall")),
        )
    }];
    let scanned_mods = vec![owner, patcher];
    let indices = Indices::build(
        &scanned_mods,
        &HashSet::new(),
        &active_of(&scanned_mods),
        &BTreeSet::new(),
    );
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_target_edges(
            &scanned_mods,
            &indices,
            &active,
            &name_map,
            &BTreeMap::new()
        )
        .is_empty()
    );
}

#[test]
fn may_require_edge_points_at_the_named_active_mod() {
    use crate::domain::DefEntry;
    let mut gated = scanned(bare_mod("gated", "Gated"), vec![]);
    gated.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "X".into(),
        may_require: vec!["other.mod".to_string()],
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let other = scanned(bare_mod("other.mod", "Other"), vec![]);
    let scanned_mods = vec![gated, other];
    let active = active_of(&scanned_mods);

    let edges = may_require_edges(&scanned_mods, &active);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("gated"));
    assert_eq!(edges[0].before, ModId::new("other.mod"));
    assert_eq!(edges[0].kind, EdgeKind::MayRequire);
}

// -- patch_injected_node_edges -------------------------------------

fn injecting_op(def_type: &str, def_name: &str, injected: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_types: BTreeSet::from([injected.to_string()]),
        ..patch_op(
            "PatchOperationAdd",
            Some(def_name_target(def_type, def_name)),
        )
    }
}

fn selecting_op(xpath: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(xpath.to_string()),
        ..patch_op("PatchOperationAdd", None)
    }
}

#[test]
fn patch_injected_node_edge_when_the_type_is_injected_and_never_written_inline() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_op("ThingDef", "X", "Example.Weapons.HeavyWeapon")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_op(
        r#"Defs/ThingDef[defName="Y"]/modExtensions/li[@Class="Example.Weapons.HeavyWeapon"]"#,
    )];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Hard);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("Example.Weapons.HeavyWeapon"),
        "subject must name the injected class (Report schema 4)"
    );
}

/// A `PatchOperationReplace` whose own `<value>` introduces an element
/// carrying a `Class` attribute is recognized as an injector, exactly
/// like an `Add` — verified here through the *real* extractor
/// (`extract::patches::walk`), not a hand-built `PatchOp`, since the
/// fact under test (`collect_class_strings` running unconditionally
/// over any mutating op's `<value>`, `Add` or `Replace` alike — see
/// `extract::patches`'s own `injected_types` construction) lives one
/// layer below what a hand-set `injecting_op(...)` fixture would ever
/// exercise. The class-string injector pass
/// (`injected_types`/`emit_patch_injected_node_edge`) never gates on
/// operation class — only the *other* pass, the element-*path* one
/// (`extract::patches::injected_paths_of`, gated by
/// `APPEND_SEMANTICS_SUFFIXES`), excludes `Replace`, and it
/// must stay excluded: a `PatchOperationReplace`'s `<value>` **is**
/// the replaced node, so its own top-level element tag is
/// structurally the target's own tag (that function's own doc comment
/// has the real `alienRace/alienRace` doubled-path example) —
/// widening that pass to `Replace` would not add coverage, it would
/// produce exactly the false paths this exclusion exists to prevent,
/// for a class attribute this class-string pass already covers
/// correctly. A selector predicate naming a *bare*,
/// non-namespace-qualified class never reaches this pass at all, for a
/// reason unrelated to `Add`/`Replace`: see
/// `xpath_expr::selected_classes`'s own dot-requirement.
#[test]
fn patch_injected_node_edge_recognizes_a_replace_introduced_class_exactly_like_add() {
    use crate::extract::patches;
    use std::sync::Arc;

    let file = Arc::from(PathBuf::from("test.xml"));
    let injector_xml = br#"<Patch>
          <Operation Class="PatchOperationReplace">
            <xpath>Defs/ThingDef[defName="X"]/comps/li[@Class="Old.Namespaced.Class"]</xpath>
            <value><li Class="New.Namespaced.Class"><v>1</v></li></value>
          </Operation>
        </Patch>"#;
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = patches::walk(injector_xml, &file).unwrap();
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_op(
        r#"Defs/ThingDef[defName="Y"]/modExtensions/li[@Class="New.Namespaced.Class"]"#,
    )];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Hard);
    assert_eq!(edges[0].subject.as_deref(), Some("New.Namespaced.Class"));
}

/// The "inline writer" false positive shape: injected by one mod,
/// selected by another, written inline by a third -> demoted to
/// `UsesType`, never `PatchInjectedNode`.
#[test]
fn patch_injected_node_demotes_to_uses_type_when_written_inline_elsewhere() {
    use crate::domain::DefEntry;
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_op("ThingDef", "X", "Example.Weapons.HeavyWeapon")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_op(
        r#"Defs/ThingDef[defName="Y"]/modExtensions/li[@Class="Example.Weapons.HeavyWeapon"]"#,
    )];
    let mut inline_writer = scanned(bare_mod("inline_writer", "InlineWriter"), vec![]);
    inline_writer.inline_types = BTreeSet::from(["Example.Weapons.HeavyWeapon".to_string()]);
    inline_writer.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "Z".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let scanned_mods = vec![injector, selector, inline_writer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::UsesType);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Awareness);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("Example.Weapons.HeavyWeapon"),
        "the demoted UsesType sibling must still carry the same injected class as subject"
    );
}

#[test]
fn patch_injected_node_produces_no_edge_when_nothing_injects_the_selected_type() {
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_op(
        r#"Defs/ThingDef[defName="Y"]/modExtensions/li[@Class="Un.Injected.Type"]"#,
    )];
    let scanned_mods = vec![selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map).is_empty());
}

/// Rule 1's "ambiguous injector" demotion, generalized to a mod being
/// ambiguous with itself (the real
/// `example.boneyard`/`example.progression.furniture` shape): the selector's
/// own patch also injects the class it selects, so the *other* mod's
/// injection of the same string can't be named as the sole, provably-required
/// cause of the node — the selector's own patch could have created it
/// regardless of the other mod's.
#[test]
fn patch_injected_node_demotes_to_uses_type_when_the_selector_also_injects_the_class() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_op("ThingDef", "X", "Some.Injected.Class")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![
        injecting_op("ThingDef", "Y", "Some.Injected.Class"),
        selecting_op(
            r#"Defs/ThingDef[defName="Z"]/modExtensions/li[@Class="Some.Injected.Class"]"#,
        ),
    ];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::UsesType);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Awareness);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("Some.Injected.Class"),
        "the demoted UsesType sibling must still carry the same injected class as subject"
    );
}

/// Rule 2: the selecting op and the sole injector's op both carry a
/// `DefTarget`, but for different defs — the injected node lives
/// under an unrelated def, so selecting the same class string
/// elsewhere isn't evidence the selector's own def depends on *this*
/// injection.
#[test]
fn patch_injected_node_demotes_to_uses_type_when_def_targets_disagree() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_op(
        "ThingDef",
        "InjectorTarget",
        "Some.Injected.Class",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        target: Some(def_name_target("ThingDef", "UnrelatedTarget")),
        ..selecting_op(
            r#"Defs/ThingDef[defName="UnrelatedTarget"]/modExtensions/li[@Class="Some.Injected.Class"]"#,
        )
    }];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::UsesType);
}

/// Positive control for rule 2: when the selecting op and the
/// injecting op agree on `(def_type, def_name)`, the `Hard` edge
/// still fires — the rule only demotes on an actual disagreement,
/// never merely because both ops happen to carry a target.
#[test]
fn patch_injected_node_edge_when_def_targets_agree() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_op(
        "ThingDef",
        "SharedTarget",
        "Some.Injected.Class",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        target: Some(def_name_target("ThingDef", "SharedTarget")),
        ..selecting_op(
            r#"Defs/ThingDef[defName="SharedTarget"]/modExtensions/li[@Class="Some.Injected.Class"]"#,
        )
    }];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

// -- patch_injected_node_edges, second pass (node paths / injected
// defs) -----------------------------------------------------------

fn injecting_path_op(path: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from([path.to_string()]),
        ..patch_op("PatchOperationAdd", None)
    }
}

fn selecting_path_op(target: DefTarget) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        target: Some(target),
        ..patch_op("PatchOperationAdd", None)
    }
}

fn def_target_with_sub_path(def_type: &str, def_name: &str, sub_path: &str) -> DefTarget {
    DefTarget {
        sub_path: Some(sub_path.to_string()),
        ..def_name_target(def_type, def_name)
    }
}

/// The element-injection shape: a selecting op whose own target path
/// exactly matches another mod's injected element, and no active mod
/// writes that exact node inline anywhere. Doubles as a regression pin:
/// `ThingDef/Human/alienRace` is the alien-race framework's own real injection — `alienRace`
/// is *not* inline in vanilla `Human` — and this exact shape must stay
/// `Hard` while the `inline_node_paths` check below demotes the cases
/// that *are* genuinely inline.
#[test]
fn path_pass_edge_when_an_injected_element_is_matched_by_a_later_selector() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/Human/alienRace")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef",
        "Human",
        "alienRace",
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("ThingDef/Human/alienRace")
    );
}

/// A real-install shape: a selecting op whose
/// own sub_path ends in a `Class`-keyed `<li>` predicate reaches the
/// node another mod's `Add` injects there, via the normalized
/// predicate-li key — even when the two mods wrote the quote character
/// differently (double- vs. single-quoted), which a raw string
/// comparison would have missed.
#[test]
fn predicate_li_class_shape_produces_a_hard_edge_despite_differing_quote_style() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(
        "PreceptDef/A/comps/li[@Class=\"Example.RoomRequirement\"]",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "PreceptDef",
        "A",
        r#"comps/li[@Class='Example.RoomRequirement']"#,
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// Another real-install shape: a bare-text
/// `<li>` predicate (`text()="v"`, the "plain defName reference" idiom)
/// reaches its injector too.
#[test]
fn predicate_li_text_shape_produces_a_hard_edge() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(
        "ThingDef/A/researchPrerequisites/li[text()=\"SomeResearch\"]",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef",
        "A",
        r#"researchPrerequisites/li[text()="SomeResearch"]"#,
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// A selector whose predicate value doesn't match anything injected
/// produces no edge at all — precision, not a flood.
#[test]
fn predicate_li_shape_produces_no_edge_when_the_value_differs() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(
        "PreceptDef/A/comps/li[@Class=\"Example.RoomRequirement\"]",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "PreceptDef",
        "A",
        r#"comps/li[@Class="Example.SomethingElse"]"#,
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert!(edges.is_empty(), "edges: {edges:?}");
}

/// A real-install shape: `B` replaces a predicate-keyed `<li>` with a new
/// one carrying a different `Class`, and `A`'s own op selects a node
/// *through* that new `<li>` (a nested path, not the `<li>` itself) — the
/// dependency is on the intermediate `<li>` existing, found by scanning
/// every segment of `A`'s own sub_path, not only the last.
#[test]
fn predicate_li_shape_matches_an_intermediate_segment_not_only_the_last() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![injecting_path_op(
        "PreceptDef/A/comps/li[@Class=\"Example.RoomRequirement_ThingAnyOfCount\"]",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "PreceptDef",
        "A",
        r#"comps/li[@Class="Example.RoomRequirement_ThingAnyOfCount"]/things/li[text()="Column"]"#,
    ))];
    let scanned_mods = vec![replacer, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("replacer"));
}

/// A real-install shape: an op nested under a Conditional whose own
/// xpath equals the op's own (the "if it exists, then ..." idiom)
/// tolerates its own target's absence, so it must never be counted as
/// requiring another mod's own predicate-keyed `<li>` injection — real
/// case, two mods each carrying exactly this idiom over the *other
/// direction* of the same pair produces a genuine mutual dependency
/// that would otherwise fabricate an unbreakable `Hard` cycle neither
/// op's own author actually required.
#[test]
fn predicate_li_shape_skips_a_toucher_that_tolerates_its_own_targets_absence() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(
        "XenotypeDef/Highmate/genes/li[text()=\"Hair_LongOnly\"]",
    )];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![PatchOp {
        xpath: Some(
            r#"Defs/XenotypeDef[defName="Highmate"]/genes/li[text()="Hair_LongOnly"]"#.to_string(),
        ),
        conditional_xpath: Some(
            r#"Defs/XenotypeDef[defName="Highmate"]/genes/li[text()="Hair_LongOnly"]"#.to_string(),
        ),
        ..selecting_path_op(def_target_with_sub_path(
            "XenotypeDef",
            "Highmate",
            r#"genes/li[text()="Hair_LongOnly"]"#,
        ))
    }];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert!(edges.is_empty(), "edges: {edges:?}");
}

/// Runs the injected-node pass over an injector, a selector and a
/// "base" mod whose inline node hashes come from really indexing
/// `base_defs_xml` (the same path a scan takes), so the test breaks if the
/// extract side stops producing the key the edge side looks up.
fn predicate_li_edges_with_base_defs(
    injected_path: &str,
    selector_sub_path: &str,
    base_defs_xml: &str,
) -> Vec<Edge> {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(injected_path)];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "XenotypeDef",
        "A",
        selector_sub_path,
    ))];
    let mut base = scanned(bare_mod("base", "Base"), vec![]);
    base.inline_node_path_hashes = crate::extract::defs::index(
        base_defs_xml.as_bytes(),
        &std::sync::Arc::from(std::path::Path::new("base.xml")),
    )
    .expect("fixture defs parse")
    .inline_node_path_hashes;
    let scanned_mods = vec![injector, selector, base];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);
    patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map)
}

/// A `Remove` of `genes/li[text()="X"]` whose `<li>` another mod's patch
/// also writes, where a Def already ships that exact `<li>`: the Remove
/// works in either order, so the edge must be advisory, never `Hard`.
#[test]
fn predicate_li_text_shape_demotes_when_a_def_ships_the_li_inline() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/genes/li[text()=\"GeneX\"]",
        r#"genes/li[text()='GeneX']"#,
        "<Defs><XenotypeDef><defName>A</defName><genes><li>GeneX</li></genes></XenotypeDef></Defs>",
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
    assert!(edges.iter().all(|e| e.kind != EdgeKind::PatchInjectedNode));
}

/// The `Class`-attribute twin of the test above.
#[test]
fn predicate_li_class_shape_demotes_when_a_def_ships_the_li_inline() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/comps/li[@Class=\"Example.CompX\"]",
        r#"comps/li[@Class="Example.CompX"]"#,
        r#"<Defs><XenotypeDef><defName>A</defName><comps><li Class="Example.CompX"><a>1</a></li></comps></XenotypeDef></Defs>"#,
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
}

/// Positive control: when no Def ships the `<li>`, the `Hard` edge stays.
#[test]
fn predicate_li_shape_stays_hard_when_no_def_ships_the_li() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/genes/li[text()=\"GeneX\"]",
        r#"genes/li[text()="GeneX"]"#,
        "<Defs><XenotypeDef><defName>A</defName><genes><li>OtherGene</li></genes></XenotypeDef></Defs>",
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
}

/// A selector step with two predicates (`li[@Class="X"][thingDef="Column"]`)
/// keys on its first recognized predicate only; an inline `<li Class="X">`
/// proves nothing about the second, so the edge stays `Hard`.
#[test]
fn predicate_li_multi_predicate_step_stays_hard_despite_an_inline_li_with_the_first_predicate() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/comps/li[@Class=\"Example.CompX\"]",
        r#"comps/li[@Class="Example.CompX"][thingDef="Column"]"#,
        r#"<Defs><XenotypeDef><defName>A</defName><comps><li Class="Example.CompX"><thingDef>Other</thingDef></li></comps></XenotypeDef></Defs>"#,
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// A read-through step (`li[@Class="X"]/things/li[text()="Y"]`): an inline
/// `<li Class="X">` proves only that the container exists, not the node the
/// op reaches through it, so the edge stays `Hard`.
#[test]
fn predicate_li_read_through_step_stays_hard_despite_an_inline_container_li() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/comps/li[@Class=\"Example.CompX\"]",
        r#"comps/li[@Class="Example.CompX"]/things/li[text()="Y"]"#,
        r#"<Defs><XenotypeDef><defName>A</defName><comps><li Class="Example.CompX"><things><li>Z</li></things></li></comps></XenotypeDef></Defs>"#,
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// The inline key keeps its full prefix: the same `li` value under a
/// different def never demotes.
#[test]
fn predicate_li_stays_hard_when_the_same_li_is_inline_under_a_different_def() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/genes/li[text()=\"GeneX\"]",
        r#"genes/li[text()="GeneX"]"#,
        "<Defs><XenotypeDef><defName>B</defName><genes><li>GeneX</li></genes></XenotypeDef></Defs>",
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// ... and under a different container of the same def.
#[test]
fn predicate_li_stays_hard_when_the_same_li_is_inline_under_a_different_container() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/genes/li[text()=\"GeneX\"]",
        r#"genes/li[text()="GeneX"]"#,
        "<Defs><XenotypeDef><defName>A</defName><otherGenes><li>GeneX</li></otherGenes></XenotypeDef></Defs>",
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// The game's `text()="GeneX"` does not match `<li>  GeneX </li>`, so a
/// padded inline `<li>` must not demote the edge.
#[test]
fn predicate_li_text_shape_stays_hard_when_the_inline_li_text_is_padded() {
    let edges = predicate_li_edges_with_base_defs(
        "XenotypeDef/A/genes/li[text()=\"GeneX\"]",
        r#"genes/li[text()="GeneX"]"#,
        "<Defs><XenotypeDef><defName>A</defName><genes><li>  GeneX </li></genes></XenotypeDef></Defs>",
    );

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// An element-injection path that a *different* active mod already
/// writes inline (e.g. `ThingDef/MealNutrientPaste/ingestible`,
/// shipped by Core) must demote to advisory
/// `PatchSelectsInjectedNode`, not assert a `Hard` requirement the
/// patch didn't actually create.
#[test]
fn path_pass_demotes_to_selects_injected_node_when_element_path_exists_inline() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/MealNutrientPaste/ingestible")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef",
        "MealNutrientPaste",
        "ingestible",
    ))];
    let mut inline_writer = scanned(bare_mod("core", "Core"), vec![]);
    inline_writer.inline_node_path_hashes = HashSet::from([crate::domain::hash_node_path(
        "ThingDef/MealNutrientPaste/ingestible",
    )]);
    let scanned_mods = vec![injector, selector, inline_writer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("ThingDef/MealNutrientPaste/ingestible")
    );
}

/// Bounded conservatism: a path deeper than
/// `extract::defs::MAX_INLINE_NODE_PATH_DEPTH` was never indexed at
/// all (regardless of whether it's genuinely inline), so it's treated
/// as unknown and demoted — never trusted as "not found, therefore
/// Hard".
#[test]
fn path_pass_demotes_to_selects_injected_node_when_path_deeper_than_indexed_depth() {
    const {
        assert!(
            crate::extract::defs::MAX_INLINE_NODE_PATH_DEPTH < 4,
            "this test's own path must actually exceed the indexed depth"
        );
    }
    let deep_path = "ThingDef/Wall/a/b/c/d";
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(deep_path)];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef", "Wall", "a/b/c/d",
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
}

/// Depth 3 is the inclusive boundary
/// (`element_injection_depth(path) <= MAX_INLINE_NODE_PATH_DEPTH`) —
/// pins the other direction from the test above so a future `<=` ->
/// `<` slip can't silently over-demote every boundary path. A
/// depth-3 path not actually present in the inline index still
/// asserts `Hard`, proving the index is genuinely consulted at the
/// boundary rather than skipped outright.
#[test]
fn path_pass_asserts_hard_at_exactly_the_indexed_depth_when_not_found_inline() {
    const {
        assert!(
            crate::extract::defs::MAX_INLINE_NODE_PATH_DEPTH == 3,
            "this test's own path depth must match the indexed bound exactly"
        );
    }
    let boundary_path = "ThingDef/Wall/a/b/c";
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op(boundary_path)];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef", "Wall", "a/b/c",
    ))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// The whole-`<Defs>`-root shape: a selecting op targeting the
/// injected def by name, with no inline owner of that name anywhere.
#[test]
fn path_pass_edge_when_a_whole_injected_def_is_matched_by_a_later_selector() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/NewThing")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_name_target("ThingDef", "NewThing"))];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/NewThing"));
}

/// Whole-def shape only: an inline-authored def of the identical name
/// demotes the edge — the def could exist independent of the patch.
#[test]
fn path_pass_demotes_to_selects_injected_node_when_whole_def_has_inline_owner() {
    use crate::domain::DefEntry;
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/NewThing")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_name_target("ThingDef", "NewThing"))];
    let mut inline_writer = scanned(bare_mod("inline_writer", "InlineWriter"), vec![]);
    inline_writer.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "NewThing".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let scanned_mods = vec![injector, selector, inline_writer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/NewThing"));
}

/// More than one active mod injecting the identical path: no single
/// injector can be named with confidence, so both demote.
#[test]
fn path_pass_demotes_to_selects_injected_node_when_multiple_mods_inject_same_path() {
    let mut injector_one = scanned(bare_mod("injector_one", "InjectorOne"), vec![]);
    injector_one.patch_ops = vec![injecting_path_op("ThingDef/Human/alienRace")];
    let mut injector_two = scanned(bare_mod("injector_two", "InjectorTwo"), vec![]);
    injector_two.patch_ops = vec![injecting_path_op("ThingDef/Human/alienRace")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef",
        "Human",
        "alienRace",
    ))];
    let scanned_mods = vec![injector_one, injector_two, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|e| e.kind == EdgeKind::PatchSelectsInjectedNode)
    );
}

/// The selector's own patch also injects the path it selects: its own
/// patch could have created the node regardless of the other mod's,
/// so even a single other injector is demoted.
#[test]
fn path_pass_demotes_to_selects_injected_node_when_selector_also_injects_path() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/Human/alienRace")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![
        injecting_path_op("ThingDef/Human/alienRace"),
        selecting_path_op(def_target_with_sub_path("ThingDef", "Human", "alienRace")),
    ];
    let scanned_mods = vec![injector, selector];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("selector"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
}

/// The inline-node index represents a `[@Name="…"]` target; without
/// that, this always reads "absent" and asserts a false `Hard` edge.
/// Real subject: `ThingDef[@Name="MechGestatorBase"]`
/// (`Data/Biotech/Defs/ThingDefs_Buildings/Buildings_Production.xml`,
/// `Abstract="True"`, ships `<comps>` inline; the only contributing
/// op's own guard, `[@Name="MechGestatorBase" and not(comps)]`, never
/// fires on 1.6). `Selector::NameAttr` must demote exactly like
/// `Selector::DefName` already does.
#[test]
fn path_pass_demotes_to_selects_injected_node_when_name_attr_template_ships_inline() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/@MechGestatorBase/comps")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(DefTarget {
        def_type: "ThingDef".to_string(),
        def_name: "MechGestatorBase".to_string(),
        selector: Selector::NameAttr,
        sub_path: Some("comps".to_string()),
    })];
    let mut inline_writer = scanned(bare_mod("core", "Core"), vec![]);
    inline_writer.inline_node_path_hashes = HashSet::from([crate::domain::hash_node_path(
        "ThingDef/@MechGestatorBase/comps",
    )]);
    let scanned_mods = vec![injector, selector, inline_writer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("ThingDef/@MechGestatorBase/comps")
    );
}

/// Real installs target some `(def_type, def_name)` keys under both
/// selectors, `ThingDef/Human` among them: a `[@Name="X"]` template's
/// own inline nodes must never leak into a
/// `[defName="X"]` def's own "pre-exists inline" check, even when
/// both share the literal name — reuses the alien-race framework's
/// `ThingDef/Human/alienRace` regression fixture, adding a decoy
/// `[@Name="Human"]` template that ships `alienRace` inline; the
/// `defName="Human"` edge must stay `Hard` regardless.
#[test]
fn path_pass_selector_aware_index_does_not_let_a_name_attr_decoy_demote_a_def_name_target() {
    let mut injector = scanned(bare_mod("injector", "Injector"), vec![]);
    injector.patch_ops = vec![injecting_path_op("ThingDef/Human/alienRace")];
    let mut selector = scanned(bare_mod("selector", "Selector"), vec![]);
    selector.patch_ops = vec![selecting_path_op(def_target_with_sub_path(
        "ThingDef",
        "Human",
        "alienRace",
    ))];
    let mut decoy = scanned(bare_mod("decoy", "Decoy"), vec![]);
    decoy.inline_node_path_hashes =
        HashSet::from([crate::domain::hash_node_path("ThingDef/@Human/alienRace")]);
    let scanned_mods = vec![injector, selector, decoy];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_injected_node_edges(&scanned_mods, &indices, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInjectedNode);
}

/// The whole-def shape's own `@`-prefix routing: a `[@Name="X"]`
/// whole-def target must consult
/// `indices.template_owners`, not `indices.def_owners` (which is
/// `defName`-keyed and would never hold a template). Exercised via a
/// direct call — `injected_path_owners` is never actually populated
/// with an `@`-prefixed *two*-segment key by any real producer today
/// (the whole-`<Defs>`-root injection shape is always `defName`-only,
/// see [`PatchOp::injected_paths`]'s own doc comment), so this pins
/// the routing itself rather than an end-to-end scenario. Loosely
/// modeled on the real `ExampleVehicles.VehicleDef/BaseVehiclePawn` subject.
#[test]
fn emit_path_injected_node_edge_whole_def_name_attr_consults_template_owners() {
    use crate::domain::TemplateEntry;
    let mut template_owner = scanned(bare_mod("template_owner", "TemplateOwner"), vec![]);
    template_owner.templates = vec![TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: "ExampleVehicles.VehicleDef".to_string(),
        name: "BaseVehiclePawn".to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];
    let scanned_mods = vec![template_owner];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let mut edges = Vec::new();
    let mut seen: SeenEdges = HashSet::new();
    let mut injected_path_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
    injected_path_owners.insert(
        "ExampleVehicles.VehicleDef/@BaseVehiclePawn".to_string(),
        vec![ModId::new("injector")],
    );

    emit_path_injected_node_edge(
        &mut edges,
        &mut seen,
        &indices,
        &injected_path_owners,
        &ModId::new("selector"),
        "ExampleVehicles.VehicleDef/@BaseVehiclePawn",
        InlineCheck::Applicable,
    );

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchSelectsInjectedNode);
}

// -- patch_target_edges + injected_def_owners ------------------------

/// A patch that touches *inside* a whole-def-injected def (not the
/// exact top-level path the second pass above matches) still finds an
/// owner, at `Awareness` strength — the gap `injected_def_owners`
/// wiring into `patch_target_edges` closes.
#[test]
fn patch_target_edge_fires_for_a_sub_path_inside_a_whole_def_patch_injected_owner() {
    let injector = scanned(bare_mod("injector", "Injector"), vec![]);
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![patch_op(
        "PatchOperationReplace",
        Some(def_target_with_sub_path(
            "ThingDef",
            "NewThing",
            "statBases",
        )),
    )];
    let scanned_mods = vec![injector, patcher];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(&scanned_mods);
    let injected_owners = BTreeMap::from([(
        ("ThingDef".to_string(), "NewThing".to_string()),
        vec![ModId::new("injector")],
    )]);

    let edges = patch_target_edges(
        &scanned_mods,
        &indices,
        &active,
        &name_map,
        &injected_owners,
    );

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("patcher"));
    assert_eq!(edges[0].before, ModId::new("injector"));
    assert_eq!(edges[0].kind, EdgeKind::PatchTargetsDef);
}

// -- patch_removed_node_edges ----------------------------------------

fn remove_op(xpath: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(xpath.to_string()),
        ..patch_op("PatchOperationRemove", None)
    }
}

fn touching_op(class: &str, xpath: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some(xpath.to_string()),
        ..patch_op(class, None)
    }
}

/// A bare Remove and a Replace of the *exact same* node, neither nested
/// under a Sequence or Conditional: the canonical cosmetic shape
/// — whichever order they run in, nothing of the Replace survives, so
/// this is `PatchRemovedNodeCosmetic`, not `PatchRemovedNode`.
#[test]
fn patch_removed_node_edge_for_a_plain_remove_is_cosmetic() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("remover"));
    assert_eq!(edges[0].before, ModId::new("other"));
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/Wall/comps"));
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// An "if exists, then remove" Conditional — its own xpath equals the
/// Remove's — is unconditional in effect and still emits, still cosmetic
/// (the Conditional wraps the *remover*, not the toucher, which is what
/// items a-d actually check).
#[test]
fn patch_removed_node_edge_fires_for_an_if_exists_conditional_remove() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        ..remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)
    }];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// A genuinely conditional remove — the enclosing Conditional's own
/// xpath *differs* from the Remove's — emits no edge at all.
#[test]
fn patch_removed_node_edge_is_suppressed_for_a_genuinely_conditional_remove() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]"#.to_string()),
        ..remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)
    }];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A whole-def remove (`P = None`) covers every sub_path, including a
/// deeply nested child one.
#[test]
fn patch_removed_node_edge_for_a_whole_def_remove_covers_a_child_sub_path() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/comps/li"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/Wall"));
    // Still cosmetic: the whole def (including the toucher's own replaced
    // child, wherever it lands) is gone either order.
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// A mod's own Remove is never ordered against itself, even when the
/// same mod also has another op touching the identical node.
#[test]
fn patch_removed_node_edge_excludes_the_removers_own_mod() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#),
        touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        ),
    ];
    let scanned_mods = vec![remover];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A Remove nested under an unsatisfied `PatchOperationFindMod`
/// branch never runs, so it produces no edge.
#[test]
fn patch_removed_node_edge_excludes_an_op_gated_off_by_an_unsatisfied_find_mod_branch() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_context: vec![FindModGate::AnyActive(vec!["Inactive Mod".to_string()])],
        ..remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)
    }];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    // No mod named "Inactive Mod" is active, so the gate stays closed.
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The descendant test is a real `/`-delimited path-segment
/// comparison, never a bare string prefix: `statBases` must not match
/// `statBasesExtra`.
#[test]
fn patch_removed_node_edge_requires_a_real_path_segment_match_not_a_string_prefix() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/statBases"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Wall"]/statBasesExtra"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A real content-pack-mod / progression-furniture-mod shape: two mods each
/// remove a node the other also touches, forming a mutual 2-cycle — expected
/// and fine, `break_cycles` drops one side.
#[test]
fn patch_removed_node_edges_form_a_mutual_cycle_when_two_mods_each_remove_the_others_touched_node()
{
    let mut mod_a = scanned(bare_mod("mod_a", "ModA"), vec![]);
    mod_a.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="One"]/comps"#),
        touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Two"]/comps"#,
        ),
    ];
    let mut mod_b = scanned(bare_mod("mod_b", "ModB"), vec![]);
    mod_b.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="Two"]/comps"#),
        touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="One"]/comps"#,
        ),
    ];
    let scanned_mods = vec![mod_a, mod_b];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .any(|e| e.after == ModId::new("mod_a") && e.before == ModId::new("mod_b"))
    );
    assert!(
        edges
            .iter()
            .any(|e| e.after == ModId::new("mod_b") && e.before == ModId::new("mod_a"))
    );
}

/// A disjunctive-head remove must order against a toucher of *either*
/// def it names — `patch_op_targets` (not the single `op.target`) is
/// what makes this true.
#[test]
fn patch_removed_node_edge_covers_every_def_a_disjunctive_head_names() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(
        r#"Defs/ThingDef[defName="A" or defName="B"]/comps"#,
    )];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="B"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/B/comps"));
}

/// Real installs target some `(def_type, def_name)` keys under both
/// selectors, `ThingDef/Human` among them: a `[@Name="Wall"]` template
/// touch must never order against a
/// `[defName="Wall"]` def's own remove — `Selector` is part of the
/// `touchers` key precisely so these stay two different subjects,
/// mirroring `conflicts::patch_collisions`' own `(def_type, def_name,
/// selector, sub_path)` collision key.
#[test]
fn patch_removed_node_edge_does_not_conflate_def_name_and_name_attr_selectors() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[@Name="Wall"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

// -- patch_removed_node_edges: cosmetic vs content classification --

/// A later Sequence sibling means a failure in the remover-first order
/// could skip real, later work — rule (b) fails, so the pair stays
/// `PatchRemovedNode` (content) even though the toucher's own write is
/// otherwise entirely inside the remover's region.
#[test]
fn toucher_with_later_sequence_sibling_is_content() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        sequence_tail: false,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNode);
}

/// The same non-tail shape, but the toucher is also reached through an
/// (open) `PatchOperationFindMod` branch nested inside the Sequence —
/// `sequence_tail` alone (already resolved through every wrapper by
/// `extract::patches::walk`) still decides content, regardless of the
/// FindMod gate being satisfied.
#[test]
fn toucher_nested_under_findmod_inside_sequence_with_later_sibling_is_content() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        sequence_tail: false,
        find_mod_context: vec![FindModGate::NoneActive(vec!["Nonexistent Mod".to_string()])],
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNode);
}

/// A toucher mod with *two* ops on the same key: one inside the removed
/// region, one genuinely outside it (`statBases` vs. `statBasesExtra` —
/// a real path-segment difference, not a shared key). Rule (a) is a
/// *filter*, not a per-op veto (a coordinator-reviewed correction — see
/// `toucher_op_is_cosmetic`'s own doc comment): the outside-region op is
/// unaffected by the removal, so it's simply ignored, and the pair is
/// judged on the *covering* op alone, which qualifies as cosmetic here.
#[test]
fn toucher_op_outside_removed_path_is_ignored_pair_stays_cosmetic() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![
        touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        ),
        touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/statBases"#,
        ),
    ];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// A disjunctive-head toucher (`names_single_def: false`) can write
/// outside the remover's own region under the *other* name it targets,
/// so rule (d) keeps it content even though this particular target's
/// own path is covered.
#[test]
fn multi_def_head_toucher_is_content() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        names_single_def: false,
        value_child_names: std::collections::BTreeSet::new(),
        toggle_active: true,
        value_root_names: Vec::new(),
        value_digest: None,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNode);
}

/// A toucher reached through a `NoMatch` branch whose own Conditional
/// tests a location genuinely outside the remover's own removed region
/// (a different def entirely here) — rule (c) keeps it content: nothing
/// ties this toucher's own participation to the remover's own removal.
#[test]
fn nomatch_branch_writing_elsewhere_is_content() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::NoMatch),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Elsewhere"]/foo"#.to_string()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNode);
}

// -- patch_removed_node_edges: tolerance exclusions -------------------------

/// Tolerant toucher: a toucher in the `Match` branch of a Conditional testing
/// its own exact site is skipped outright — no edge at all, cosmetic or
/// not — regardless of whether the `NoMatch` branch recreates anything:
/// ground-truthed against the decompiled engine
/// (`PatchOperationConditional.ApplyWorker` returns success for a
/// match-only Conditional whenever the tested node is simply absent).
/// This case carries a creating `NoMatch` too, since that was the
/// original, narrower shape this rule covered before the same-site case
/// was widened to drop the `conditional_nomatch_creates` requirement (see
/// `same_site_match_branch_toucher_is_tolerant_without_creating_nomatch`
/// for the case that isolates the widening).
#[test]
fn tolerant_toucher_with_creating_nomatch_is_skipped() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::Match),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        conditional_nomatch_creates: true,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// Widened tolerant toucher: the same same-site `Match`-branch shape, but with
/// no creating `NoMatch` branch at all (`conditional_nomatch_creates:
/// false`) — still tolerant. A same-site Conditional's own `Match`
/// branch silently never runs once the remover has deleted the tested
/// node, whether or not `<nomatch>` does anything, so the toucher's own
/// author already tolerated that outcome just by wrapping it in a
/// Conditional.
#[test]
fn same_site_match_branch_toucher_is_tolerant_without_creating_nomatch() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::Match),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        conditional_nomatch_creates: false,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// Widened tolerant toucher: a `Match`-branch toucher whose Conditional tests a
/// *different* site from the toucher's own op, but one that still lies
/// inside the remover's own removed region, is tolerant too — the
/// Conditional's own test goes silently absent right along with the rest
/// of the deleted subtree, no creating `NoMatch` required.
#[test]
fn match_branch_toucher_within_removed_region_is_tolerant_without_creating_nomatch() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::Match),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps/other"#.to_string()),
        conditional_nomatch_creates: false,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps/inner"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The narrower, still-gated case: the Conditional tests the toucher's
/// own xpath's *parent*, a site outside the remover's own removed
/// region — here the remover's absence doesn't make the Conditional
/// itself silently no-op (the parent may still exist), so tolerance still
/// requires a creating `NoMatch` branch.
#[test]
fn parent_xpath_match_branch_toucher_without_creating_nomatch_still_emits() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps/inner"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::Match),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        conditional_nomatch_creates: false,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps/inner"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// The same parent-xpath shape, but with a creating `NoMatch` branch —
/// tolerant, exactly as before the widening.
#[test]
fn parent_xpath_match_branch_toucher_with_creating_nomatch_is_skipped() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps/inner"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::Match),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        conditional_nomatch_creates: true,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps/inner"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A toucher reached through a `NoMatch` branch (not `Match`) is never
/// treated as tolerant even with `conditional_nomatch_creates: true` set
/// — that field is only meaningful alongside `Match`. Here the
/// Conditional's own tested region also matches the remover's, so rule
/// (c) doesn't disqualify it either: the edge fires, cosmetic.
#[test]
fn nomatch_branch_toucher_is_not_treated_as_tolerant() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_branch: Some(ConditionalBranch::NoMatch),
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]/comps"#.to_string()),
        conditional_nomatch_creates: true,
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// Folder-gated toucher: a toucher whose own loaded folder is gated `IfModActive`
/// on the remover's own mod is skipped outright — that folder was
/// written for the remover's own end state.
#[test]
fn toucher_in_folder_gated_on_remover_is_skipped() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        load_folder_gate: vec![ModId::new("remover")],
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The same folder-gate shape, but gated on a third mod instead of the
/// remover — irrelevant to this pair, so the edge still fires.
#[test]
fn toucher_in_folder_gated_on_a_third_mod_still_emits() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        load_folder_gate: vec![ModId::new("third")],
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
}

/// A multi-mod `IfModActiveAll` gate (several ids on one `load_folder_gate`)
/// still excludes the toucher when the remover's own mod is merely *one*
/// of the several named ids, not the only one.
#[test]
fn if_mod_active_all_gate_counts() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(r#"Defs/ThingDef[defName="Wall"]/comps"#)];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        load_folder_gate: vec![ModId::new("third"), ModId::new("remover")],
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// Rule (a): two mods each `Remove` the exact same node — a duplicate
/// removal. The final document is identical whichever one loads last (one
/// succeeds, the other finds nothing left to remove), so *both* directions
/// of the resulting mutual 2-cycle are cosmetic, not content — falls out of
/// the ordinary per-op conditions once each side's own `Remove` is itself
/// the covering op (no special-casing needed beyond the item-6 rules
/// already in place). A real install ships exactly this shape: two mods
/// both `Remove` the identical `TerrainDef[@Name="WaterDeepBase"]/changeable`.
#[test]
fn duplicate_remove_toucher_is_cosmetic() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![remove_op(
        r#"Defs/TerrainDef[@Name="WaterDeepBase"]/changeable"#,
    )];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![remove_op(
        r#"Defs/TerrainDef[@Name="WaterDeepBase"]/changeable"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 2, "a duplicate removal forms a mutual 2-cycle");
    assert!(
        edges
            .iter()
            .all(|edge| edge.kind == EdgeKind::PatchRemovedNodeCosmetic)
    );
}

/// Rule (b), exact-site shape: the remover's own mod removes a node, then
/// re-adds the identical node in a *later* op of its own — the "Remove,
/// then `AddOrReplace`" idiom. The region is never permanently absent, so
/// no edge is emitted at all, even though a genuine toucher exists.
#[test]
fn remover_that_recreates_its_own_removed_node_at_the_exact_site_emits_no_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#),
        touching_op(
            "RR.PatchOperationAddOrReplace",
            r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
        ),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "the remover's own later AddOrReplace recreates the site, so it was never really a remover"
    );
}

/// Rule (b) counts only a recreate that runs *after* the remover: the same
/// own-mod `AddOrReplace` placed *before* the `Remove` is undone by it, so
/// the region stays absent and the toucher's edge still fires.
#[test]
fn remover_whose_own_recreate_runs_before_it_still_emits_the_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![
        touching_op(
            "RR.PatchOperationAddOrReplace",
            r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
        ),
        remove_op(r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert!(
        edges
            .iter()
            .any(|edge| edge.after.as_str() == "remover" && edge.before.as_str() == "other"),
        "an own-mod recreate that runs before the remover is removed by it, got {edges:?}"
    );
}

/// Rule (b), parent-recreate shape — the exact real-install structure:
/// the remover removes `FloodLight/researchPrerequisites`, then its own
/// later `RR.PatchOperationAddOrReplace` targets the *def itself*
/// (`ThingDef[defName="FloodLight"]`, no `sub_path`) with a `<value>`
/// naming `researchPrerequisites` — "add or replace this child", which the
/// op's own `value_child_names` records. No edge, same reasoning as the
/// exact-site shape.
#[test]
fn remover_that_recreates_its_own_removed_node_via_a_parent_targeting_op_emits_no_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    let mut recreate = touching_op(
        "RR.PatchOperationAddOrReplace",
        r#"Defs/ThingDef[defName="FloodLight"]"#,
    );
    recreate.value_child_names = BTreeSet::from(["researchPrerequisites".to_string()]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#),
        recreate,
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "the remover's own later parent-targeting AddOrReplace names the \
         removed child in its own value_child_names, so it recreates the site"
    );
}

/// Rule (b) parent-recreate shape is precise about the child name: a later
/// parent-targeting creating op whose own `value_child_names` names a
/// *different* child must never suppress the edge.
#[test]
fn remover_recreate_parent_shape_requires_the_exact_child_name() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    let mut recreate = touching_op(
        "RR.PatchOperationAddOrReplace",
        r#"Defs/ThingDef[defName="FloodLight"]"#,
    );
    recreate.value_child_names = BTreeSet::from(["glowRadius".to_string()]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#),
        recreate,
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
}

/// Rule (b), whole-def shape: the remover removes a whole def, then injects
/// a brand-new def of the identical name later in its own patch set — the
/// "Remove the whole def, then inject a fresh copy" idiom. No edge, same
/// reasoning as the exact-site shape.
#[test]
fn remover_that_recreates_a_whole_removed_def_emits_no_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    let mut recreate = touching_op("PatchOperationAdd", "Defs");
    recreate.injected_paths = BTreeSet::from(["ThingDef/Table_RoyalDresser".to_string()]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="Table_RoyalDresser"]"#),
        recreate,
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Table_RoyalDresser"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "the remover's own later whole-def injection recreates Table_RoyalDresser, so it was never really a remover"
    );
}

/// Rule (b)'s whole-def shape also counts only an injection that runs
/// *after* the whole-def remover.
#[test]
fn remover_whose_own_whole_def_injection_runs_before_it_still_emits_the_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    let mut injection = touching_op("PatchOperationAdd", "Defs");
    injection.injected_paths = BTreeSet::from(["ThingDef/Table_RoyalDresser".to_string()]);
    remover.patch_ops = vec![
        injection,
        remove_op(r#"Defs/ThingDef[defName="Table_RoyalDresser"]"#),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="Table_RoyalDresser"]/comps"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert!(
        edges
            .iter()
            .any(|edge| edge.after.as_str() == "remover" && edge.before.as_str() == "other"),
        "an own-mod injection that runs before the whole-def remover is removed by it, got {edges:?}"
    );
}

/// Rule (b) is conservative: a later op of the remover's own mod that
/// creates something *else* (a different def) must never suppress the
/// edge — only an exact site/def match counts.
#[test]
fn remover_recreating_an_unrelated_site_still_emits_the_edge() {
    let mut remover = scanned(bare_mod("remover", "Remover"), vec![]);
    remover.patch_ops = vec![
        remove_op(r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#),
        touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="SomeOtherDef"]/researchPrerequisites"#,
        ),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.patch_ops = vec![touching_op(
        "PatchOperationReplace",
        r#"Defs/ThingDef[defName="FloodLight"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![remover, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
}

// -- patch_removed_node_edges, Replace-remover pass ------------------
// A node-keeping `PatchOperationReplace` treated as a remover of
// everything strictly below the node it keeps, for a restricted,
// "reading/modifying" toucher class (`Remove`/`Replace`/`Insert`) — the
// real-install stove/kitchen shape.

fn replace_keeping_op(xpath: &str, kept_tag: &str) -> PatchOp {
    PatchOp {
        value_root_names: vec![kept_tag.to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op("PatchOperationReplace", xpath)
    }
}

/// The stove shape itself: `Kitchen` replaces the whole
/// `researchPrerequisites` list (keeping its own tag), and `Production`'s
/// `Remove` targets a predicate-`<li>` that only existed in the *old*
/// list. `Production` must load before `Kitchen`. Naturally cosmetic
/// (the same reasoning generalizes here too — a `Remove` that finds
/// nothing simply does nothing, so the final document is `Kitchen`'s own
/// fresh content either order; only whether `Production`'s own op logs a
/// failure changes), the default fixture shape (`sequence_tail: true`
/// etc.).
#[test]
fn patch_removed_node_edge_replace_remover_emits_for_a_reading_toucher_below_it() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![remove_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
    )];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].after, ModId::new("kitchen"));
    assert_eq!(edges[0].before, ModId::new("production"));
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNodeCosmetic);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("ThingDef/Stove/researchPrerequisites")
    );
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// The same shape, but `Insert` instead of `Remove` — its own required
/// anchor is exactly as exposed to `Kitchen`'s old subtree being wiped.
#[test]
fn patch_removed_node_edge_replace_remover_emits_for_an_insert_toucher_at_a_missing_anchor() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![touching_op(
        "PatchOperationInsert",
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
    )];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].after, ModId::new("kitchen"));
    assert_eq!(edges[0].before, ModId::new("production"));
}

/// Cosmetic classification generalizes the same way: a covering toucher
/// with a *later* Sequence sibling of its own (`sequence_tail: false`)
/// makes the pair content, not cosmetic, exactly like the `Remove`-remover
/// pass already requires.
#[test]
fn patch_removed_node_replace_remover_is_content_when_the_toucher_has_a_later_sequence_sibling() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![PatchOp {
        sequence_tail: false,
        ..remove_op(
            r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
        )
    }];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_removed_node_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::PatchRemovedNode);
}

/// The exclusion the two mechanisms need to stay non-contradictory:
/// `Kitchen`'s own replacement value recreates a `<li>` at exactly the
/// predicate identity `Production`'s `Remove` needs (`injected_paths`, the
/// same normalized key the predicate-`<li>` producer computes) — `Production`
/// doesn't need to run first at all, so no edge.
#[test]
fn patch_removed_node_replace_remover_excludes_a_toucher_the_replacement_recreates() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        injected_paths: BTreeSet::from([
            "ThingDef/Stove/researchPrerequisites/li[text()=\"ExampleManufacturing\"]".to_string(),
        ]),
        ..replace_keeping_op(
            r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
            "researchPrerequisites",
        )
    }];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![remove_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
    )];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "the replacement's own new value recreates the exact li the toucher needs"
    );
}

/// The tolerant-toucher exclusion still applies: a `Match`-branch toucher
/// whose own Conditional tests its own exact site tolerates that site
/// being gone, so it's never counted towards this pass either.
#[test]
fn patch_removed_node_replace_remover_skips_a_tolerant_toucher() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![PatchOp {
        conditional_xpath: Some(
            r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#
                .to_string(),
        ),
        conditional_branch: Some(ConditionalBranch::Match),
        ..remove_op(
            r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
        )
    }];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "a Match-branch toucher whose own Conditional tests its own site tolerates the site's absence"
    );
}

/// The shared touchers index already excludes a `toggle_active: false`
/// op entirely, so a toucher sitting under a default-off mod-setting
/// toggle never reaches this pass either.
#[test]
fn patch_removed_node_replace_remover_skips_an_inactive_toggle_toucher() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![PatchOp {
        toggle_active: false,
        ..remove_op(
            r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites/li[text()="ExampleManufacturing"]"#,
        )
    }];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "a toucher under a default-off mod-setting toggle never runs"
    );
}

/// Class-disjointness with the discarded-addition pass: an additive toucher
/// (`Add`) below the replaced region is never picked up by this pass — that
/// is `replace_discards_addition`'s own domain, never this one's, so the two
/// mechanisms can never fire in opposite directions for the same op.
#[test]
fn patch_removed_node_replace_remover_never_picks_up_an_additive_class_toucher() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
    )];
    let scanned_mods = vec![replacer, adder];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "an additive toucher belongs to the additive-toucher pass, never this one"
    );
}

/// A toucher whose own `sub_path` names the *exact same* node the
/// `Replace` keeps — never a descendant of it — needs no order at all: a
/// node-keeping `Replace`'s own target node is guaranteed to still exist
/// afterward (a plain tag lookup, not a content-based predicate), so a
/// second `Replace` of that identical node resolves identically either
/// order (a real-install shape: two mods each `Replace` the same
/// `researchPrerequisites` list with their own different content —
/// whichever loads last simply wins, a content question the merge
/// evaluator already owns, never a failure either order).
#[test]
fn patch_removed_node_replace_remover_never_fires_for_a_toucher_at_the_exact_same_node() {
    let mut replacer = scanned(bare_mod("kitchen", "Kitchen"), vec![]);
    replacer.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let mut toucher = scanned(bare_mod("production", "Production"), vec![]);
    toucher.patch_ops = vec![replace_keeping_op(
        r#"Defs/ThingDef[defName="Stove"]/researchPrerequisites"#,
        "researchPrerequisites",
    )];
    let scanned_mods = vec![replacer, toucher];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(
        patch_removed_node_edges(&scanned_mods, &active, &name_map).is_empty(),
        "the kept node's own tag always resolves regardless of order, so a same-node toucher needs no edge"
    );
}

// -- patch_invalidates_predicate_edges -------------------------------

fn replace_op(xpath: &str) -> PatchOp {
    touching_op("PatchOperationReplace", xpath)
}

/// A real install's trait-colors mod shape: `B` replaces the
/// predicate step's own key child.
#[test]
fn fires_for_a_replace_of_the_predicate_key_child() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("b"));
    assert_eq!(edges[0].before, ModId::new("a"));
    assert_eq!(edges[0].kind, EdgeKind::PatchInvalidatesPredicate);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some(r#"TraitDef/Nerves/degreeDatas/li[label="iron-willed"]/label"#)
    );
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// `B` replaces the predicated node itself (no trailing key-child
/// segment) — still fires, against `A`'s own key-child sub_path.
#[test]
fn fires_for_a_replace_of_the_predicated_node_itself() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInvalidatesPredicate);
}

/// `B` removes (not replaces) the key child — the Remove case of
/// shape 1 still fires (only shape 2, replacing the predicated node
/// itself, is restricted to `Replace`).
#[test]
fn fires_for_a_remove_of_the_predicate_key_child() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![remove_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let edges = patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::PatchInvalidatesPredicate);
}

/// A mod is never ordered against itself, even when its own second op
/// would otherwise match.
#[test]
fn no_edge_for_the_same_mod() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![
        replace_op(r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#),
        touching_op(
            "PatchOperationAdd",
            r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
        ),
    ];
    let scanned_mods = vec![b];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// `A`'s own sub_path doesn't contain the predicate step at all —
/// a different `li[...]` value, so no shared prefix.
#[test]
fn no_edge_when_as_sub_path_does_not_contain_the_step() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The predicate value differs (`"iron-willed"` vs. `"neurotic"`) —
/// a different node entirely, no shared prefix.
#[test]
fn no_edge_when_the_predicate_value_differs() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="neurotic"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A `PatchOperationAdd` as `B` is out of scope entirely — only
/// `Replace`/`Remove` can invalidate a predicate.
#[test]
fn no_edge_for_a_patch_operation_add_as_b() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// An `@attr="v"` predicate is not a plain child-element equality —
/// out of scope, no edge.
#[test]
fn no_edge_for_an_attr_predicate() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[@Class="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[@Class="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A genuinely conditional `B` (nested under a Conditional with a
/// *different* xpath) is skipped, same rule as `patch_removed_node_edges`.
#[test]
fn genuinely_conditional_b_is_skipped() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_xpath: Some(r#"Defs/TraitDef[defName="Nerves"]"#.to_string()),
        ..replace_op(r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#)
    }];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The real `example.core`/`example.doors` shape: both mods carry the
/// *identical*
/// `PatchOperationConditional[xpath = X]/PatchOperationReplace[xpath
/// = X]` idiom on the same predicated node — an "if it exists, then
/// replace it" that tolerates the node already being gone. As `A`,
/// that same tolerance means `A` never actually depends on the
/// predicate resolving, so `B`'s own replace of it must not source
/// an edge from `A`; emitting one produces a false mutual 2-cycle on a
/// real install.
#[test]
fn a_under_a_same_xpath_conditional_tolerates_the_predicates_absence_no_edge() {
    let predicated_node = r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]"#;
    let a_xpath = format!("{predicated_node}/statOffsets");
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(predicated_node)];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    // `PatchOperationAdd`, not Replace/Remove, so `A` is never itself
    // eligible as a candidate `B` — this isolates the `A`-side
    // tolerance check the fix adds, rather than also exercising (and
    // conflating with) the ordinary `B`-side gate.
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        conditional_xpath: Some(a_xpath.clone()),
        ..touching_op("PatchOperationAdd", &a_xpath)
    }];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A shape-2 fixture (`B` replaces `li[@Class="X"]` itself,
/// no trailing key child) so accepting an `@attr` predicate would
/// have to fail *this* test — `no_edge_for_an_attr_predicate` above
/// only ever exercised shape 1, where an accepted `@Class` key would
/// still fail the unrelated `key == last` check and the test would
/// pass for the wrong reason.
#[test]
fn no_edge_for_an_attr_predicate_in_shape_two() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[@Class="iron-willed"]"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[@Class="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// The prefix test is real path-segment matching, never a bare
/// string prefix — `statBases` must not match `statBasesExtra`, the
/// same rule `patch_removed_node_edge_requires_a_real_path_segment_match_not_a_string_prefix`
/// pins for `PatchRemovedNode`.
#[test]
fn no_edge_when_as_prefix_match_is_only_a_string_prefix_not_a_path_segment() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    // `li[label="iron-willed"]Extra` is a different, longer segment
    // string that merely starts with the same characters — not a
    // real descendant of `li[label="iron-willed"]`.
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]Extra/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// Shape 2 (a `Replace` of the predicated node itself, no
/// trailing key child) must never fire for a `Remove` — a same-shape
/// `Remove` is `PatchRemovedNode`'s own job, and neither shape 1 nor
/// shape 2 matches a bare predicated-node `Remove` sub_path, so this
/// producer emits nothing for it at all.
#[test]
fn no_edge_for_a_remove_of_the_predicated_node_itself() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![remove_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

/// A whole-def `A` (`sub_path: None`) reads no predicate at
/// all, so it can never be a candidate toucher for this producer.
#[test]
fn no_edge_for_a_whole_def_a() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]"#,
    )];
    let scanned_mods = vec![b, a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map).is_empty());
}

// -- parse_simple_equality_predicate --------------------------------

#[test]
fn parse_simple_equality_predicate_accepts_a_clean_equality() {
    assert_eq!(
        parse_simple_equality_predicate(r#"label="iron-willed""#),
        Some(("label", "iron-willed"))
    );
}

/// An `and` composition is rejected: `rfind` would find the *last* quote in
/// the whole bracket text, which for an `and`/`or` composition is still the
/// composed term's own trailing quote — so a "closing must be the last byte"
/// check would pass by coincidence and silently accept a composition as a
/// clean equality.
#[test]
fn parse_simple_equality_predicate_rejects_an_and_composition() {
    assert_eq!(
        parse_simple_equality_predicate(r#"label="x" and foo="y""#),
        None
    );
}

#[test]
fn parse_simple_equality_predicate_rejects_an_or_composition() {
    assert_eq!(
        parse_simple_equality_predicate(r#"label="x" or foo="y""#),
        None
    );
}

/// A genuinely empty value is still a clean equality — the fix must
/// not have overcorrected into rejecting it.
#[test]
fn parse_simple_equality_predicate_accepts_an_empty_value() {
    assert_eq!(
        parse_simple_equality_predicate(r#"label="""#),
        Some(("label", ""))
    );
}

/// Emission order is deterministic across repeated runs on the same
/// input — the workspace-wide byte-identical-report contract.
#[test]
fn emission_order_is_deterministic() {
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.patch_ops = vec![replace_op(
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/label"#,
    )];
    let mut a1 = scanned(bare_mod("a1", "A1"), vec![]);
    a1.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/statOffsets"#,
    )];
    let mut a2 = scanned(bare_mod("a2", "A2"), vec![]);
    a2.patch_ops = vec![touching_op(
        "PatchOperationAdd",
        r#"Defs/TraitDef[defName="Nerves"]/degreeDatas/li[label="iron-willed"]/skillGains"#,
    )];
    let scanned_mods = vec![b, a1, a2];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let first = patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map);
    let second = patch_invalidates_predicate_edges(&scanned_mods, &active, &name_map);

    assert_eq!(first.len(), 2);
    assert_eq!(
        first
            .iter()
            .map(|e| (&e.after, &e.before))
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|e| (&e.after, &e.before))
            .collect::<Vec<_>>()
    );
}

// -- uses_type_edges ------------------------------------------------

#[test]
fn uses_type_edge_when_the_type_namespace_matches_a_shipped_assembly_name() {
    use crate::domain::DefEntry;
    let mut user = scanned(bare_mod("user", "User"), vec![]);
    user.inline_types = BTreeSet::from(["Example.Weapons.HeavyWeapon".to_string()]);
    user.defs = vec![DefEntry {
        def_type: "ThingDef".into(),
        def_name: "X".into(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }];
    let provider = scanned(
        bare_mod("provider", "Provider"),
        vec![assembly("example.weapons", vec![])],
    );
    let scanned_mods = vec![user, provider];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = uses_type_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("user"));
    assert_eq!(edges[0].before, ModId::new("provider"));
    assert_eq!(edges[0].kind, EdgeKind::UsesType);
}

/// `Verse`/`RimWorld`/`UnityEngine`/`System` are never owners, even if
/// some mod's shipped assembly happens to be named after one of them.
#[test]
fn uses_type_never_credits_an_engine_namespace_as_an_owner() {
    let mut user = scanned(bare_mod("user", "User"), vec![]);
    user.inline_types = BTreeSet::from(["Verse.Pawn".to_string()]);
    let impostor = scanned(
        bare_mod("impostor", "Impostor"),
        vec![assembly("verse", vec![])],
    );
    let scanned_mods = vec![user, impostor];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(uses_type_edges(&scanned_mods, &indices).is_empty());
}

/// A namespace prefix shipped by more than one mod is ambiguous — no
/// single owner can be named, so no edge is produced.
#[test]
fn uses_type_produces_no_edge_when_the_assembly_name_is_ambiguous() {
    let mut user = scanned(bare_mod("user", "User"), vec![]);
    user.inline_types = BTreeSet::from(["Shared.Lib.SomeType".to_string()]);
    let a = scanned(bare_mod("a", "A"), vec![assembly("shared.lib", vec![])]);
    let b = scanned(bare_mod("b", "B"), vec![assembly("shared.lib", vec![])]);
    let scanned_mods = vec![user, a, b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(uses_type_edges(&scanned_mods, &indices).is_empty());
}

/// `dll_owner_of`'s own doc comment: an ambiguous *longest* prefix
/// match falls through to try a shorter, unique-owner prefix, rather
/// than giving up on the whole type string. `Shared.Lib.SomeType`'s
/// longest candidate prefix, `shared.lib`, is shipped by two mods
/// (ambiguous); the shorter `shared` is shipped by exactly one.
#[test]
fn uses_type_falls_through_an_ambiguous_longest_prefix_to_a_shorter_unique_one() {
    let mut user = scanned(bare_mod("user", "User"), vec![]);
    user.inline_types = BTreeSet::from(["Shared.Lib.SomeType".to_string()]);
    let a = scanned(bare_mod("a", "A"), vec![assembly("shared.lib", vec![])]);
    let b = scanned(bare_mod("b", "B"), vec![assembly("shared.lib", vec![])]);
    let short_prefix_owner = scanned(
        bare_mod("short_prefix_owner", "ShortPrefixOwner"),
        vec![assembly("shared", vec![])],
    );
    let scanned_mods = vec![user, a, b, short_prefix_owner];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = uses_type_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("user"));
    assert_eq!(edges[0].before, ModId::new("short_prefix_owner"));
}

// -- parent_template_edges -------------------------------------------

fn template_entry(name: &str, def_type: &str) -> crate::domain::TemplateEntry {
    crate::domain::TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: def_type.to_string(),
        name: name.to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }
}

fn inheriting_def(def_type: &str, def_name: &str, parent: &str) -> crate::domain::DefEntry {
    crate::domain::DefEntry {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: Some(parent.to_string()),
        locator: XmlLocator::for_test(),
    }
}

/// A `PatchOperationAdd` whose `<value>` carries `Name="{name}"` —
/// the shape that registers a template with `mod == null`.
fn template_injecting_op(name: &str) -> PatchOp {
    PatchOp {
        class: "PatchOperationAdd".to_string(),
        xpath: Some("Defs".to_string()),
        target: None,
        find_mod_context: Vec::new(),
        conditional_xpath: None,
        find_mod_names: Vec::new(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        is_mutating: true,
        injected_types: BTreeSet::new(),
        injected_paths: BTreeSet::new(),
        injected_template_names: BTreeSet::from([name.to_string()]),
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

fn parent_template_edges_of(scanned_mods: &[ScannedMod]) -> Vec<Edge> {
    let active = active_of(scanned_mods);
    let name_map = build_name_map(scanned_mods).0;
    let registrations = TemplateRegistrations::build(scanned_mods, &active, &name_map);
    parent_template_edges(scanned_mods, &active, &registrations)
}

#[test]
fn parent_template_edge_when_exactly_one_other_mod_registers_the_template() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "WallBase")];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("WallBase", "ThingDef")];

    let edges = parent_template_edges_of(&[child, provider]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("child"));
    assert_eq!(edges[0].before, ModId::new("provider"));
    assert_eq!(edges[0].kind, EdgeKind::ParentTemplate);
    assert_eq!(
        edges[0].strength(),
        EdgeStrength::Hard,
        "an unresolvable ParentName drops the def at load time"
    );
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("WallBase"),
        "subject must name the template (Report schema 4)"
    );
}

/// A vanilla-owned base template always loads first anyway — Core/DLC
/// `loadOrder` is `<=` every mod's, so `GetBestParentFor`'s
/// at-or-before test can never fail. The name is skipped outright:
/// filtering vanilla owners out of the candidate list instead would
/// still emit an edge to the *other* owner, often ordering Core itself
/// after a mod.
///
/// Exactly **one** foreign owner beside the vanilla copy, deliberately:
/// with two foreign owners the any-of
/// skip would make this pass whether or not the vanilla exclusion
/// exists, so the test would prove nothing. Here the single-owner
/// branch is live and the vanilla exclusion is the only thing
/// stopping the edge — `parent_template_edge_when_exactly_one_other_mod_registers_the_template`
/// is the same fixture without the vanilla copy, and does emit one.
#[test]
fn parent_template_produces_no_edge_when_a_vanilla_mod_also_registers_it() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "WallBase")];
    let mut core = scanned(bare_mod("core", "Core"), vec![]);
    core.info.source = Source::Core;
    core.templates = vec![template_entry("WallBase", "ThingDef")];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("WallBase", "ThingDef")];

    assert!(parent_template_edges_of(&[child, core, provider]).is_empty());
}

/// A vanilla def inheriting a
/// template only a mod registers must produce **no** edge. The engine
/// does error on it, but "this mod loads before Core" is not an
/// ordering the sorter can express — and as an `EdgeStrength::Hard`
/// edge it would be enforced at `Layer::Hard`, dragging the mod ahead
/// of vanilla. The unresolved-parent warning is what covers this.
#[test]
fn parent_template_produces_no_edge_for_a_vanilla_child() {
    let mut core = scanned(bare_mod("core", "Core"), vec![]);
    core.info.source = Source::Core;
    core.defs = vec![inheriting_def("ThingDef", "X", "ModBase")];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("ModBase", "ThingDef")];

    assert!(parent_template_edges_of(&[core, provider]).is_empty());
}

/// The child's own registration satisfies `loadOrder <= loadOrder`
/// trivially, so no order can break it. The name is skipped outright:
/// filtering the child out of the candidate list instead would emit an
/// edge to the remaining foreign owner (real shapes include
/// `example.chitinkin2`/`XFI_InsectoidCocoon`).
#[test]
fn parent_template_produces_no_edge_when_the_child_registers_the_name_itself() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "WallBase")];
    child.templates = vec![template_entry("WallBase", "ThingDef")];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.templates = vec![template_entry("WallBase", "ThingDef")];

    assert!(parent_template_edges_of(&[child, other]).is_empty());
}

/// A patch-added node has no `assetlookup` entry, so it registers
/// with `mod == null` — `GetBestParentFor`'s own fallback, which
/// every child reaches regardless of load order.
#[test]
fn parent_template_produces_no_edge_when_a_patch_registers_the_name() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "WallBase")];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("WallBase", "ThingDef")];
    let mut patcher = scanned(bare_mod("patcher", "Patcher"), vec![]);
    patcher.patch_ops = vec![template_injecting_op("WallBase")];

    assert!(parent_template_edges_of(&[child, provider, patcher]).is_empty());
}

/// Two foreign registrations make the requirement an any-of ("at
/// least one of them loads at or before the child"), which
/// `Constraint::AnyOf` cannot carry without new sorter work — see
/// `parent_template_edges`' own doc comment. No edge, deliberately,
/// rather than an arbitrary or contradictory pair of them.
#[test]
fn parent_template_produces_no_edge_with_two_foreign_registrations() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "WallBase")];
    let mut first = scanned(bare_mod("first", "First"), vec![]);
    first.templates = vec![template_entry("WallBase", "ThingDef")];
    let mut second = scanned(bare_mod("second", "Second"), vec![]);
    second.templates = vec![template_entry("WallBase", "ThingDef")];

    assert!(parent_template_edges_of(&[child, first, second]).is_empty());
}

/// A template inheriting from another mod's template is the same
/// load-time fact a def is — `TryRegister` does not distinguish them.
#[test]
fn parent_template_edge_covers_a_template_inheriting_from_another_mod() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.templates = vec![crate::domain::TemplateEntry {
        graphic_class: None,
        def_type: "ThingDef".to_string(),
        name: "ChildBase".to_string(),
        parent_name: Some("WallBase".to_string()),
        may_require: Vec::new(),
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("WallBase", "ThingDef")];

    let edges = parent_template_edges_of(&[child, provider]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("provider"));
}

/// `TryRegister` returns before registering anything when the child's
/// own `MayRequire` is unsatisfied, so such a def never resolves a
/// parent and never constrains order.
#[test]
fn parent_template_ignores_a_child_whose_may_require_is_unsatisfied() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    let mut gated = inheriting_def("ThingDef", "X", "WallBase");
    gated.may_require = vec!["not.installed".to_string()];
    child.defs = vec![gated];
    let mut provider = scanned(bare_mod("provider", "Provider"), vec![]);
    provider.templates = vec![template_entry("WallBase", "ThingDef")];

    assert!(parent_template_edges_of(&[child, provider]).is_empty());
}

/// Nobody registers the name at all: the def loads without its
/// inherited fields and an `XML error` logs, which is
/// `inheritance::broken_inheritance`'s own finding, not an edge — there
/// is no mod to order against.
#[test]
fn parent_template_produces_no_edge_when_nothing_registers_the_name() {
    let mut child = scanned(bare_mod("child", "Child"), vec![]);
    child.defs = vec![inheriting_def("ThingDef", "X", "NobodysBase")];

    assert!(parent_template_edges_of(&[child]).is_empty());
}

// -- assembly_version_precedence_edges -------------------------------

fn versioned_assembly(name: &str, major: u16) -> crate::domain::AssemblyInfo {
    crate::domain::AssemblyInfo {
        file_name: name.to_string(),
        name: name.to_string(),
        references: Vec::new(),
        version: Some(crate::domain::AssemblyVersion {
            major,
            minor: 0,
            build: 0,
            revision: 0,
        }),
        runtime_patches: Vec::new(),
        type_hierarchy: Vec::new(),
        parse_failed: false,
    }
}

#[test]
fn assembly_version_precedence_orders_the_higher_version_before_the_lower() {
    let old = scanned(
        bare_mod("old", "Old"),
        vec![versioned_assembly("customlib", 1)],
    );
    let new = scanned(
        bare_mod("new", "New"),
        vec![versioned_assembly("customlib", 2)],
    );
    let scanned_mods = vec![old, new];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = assembly_version_precedence_edges(&indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("old"));
    assert_eq!(edges[0].before, ModId::new("new"));
    assert_eq!(edges[0].kind, EdgeKind::AssemblyVersionPrecedence);
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Declared);
}

#[test]
fn assembly_version_precedence_produces_no_edge_for_equal_versions() {
    let a = scanned(bare_mod("a", "A"), vec![versioned_assembly("customlib", 1)]);
    let b = scanned(bare_mod("b", "B"), vec![versioned_assembly("customlib", 1)]);
    let scanned_mods = vec![a, b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(assembly_version_precedence_edges(&indices).is_empty());
}

/// Only the single highest-version owner gets an edge to each lower
/// owner — two owners that are both below the highest, but differ from
/// each other, get no edge between themselves: neither can be "the"
/// copy that wins at runtime, so there's no evidence about their
/// relative order. Deliberately narrower than full pairwise-by-version
/// (see the function's own doc comment for why).
#[test]
fn assembly_version_precedence_does_not_order_two_non_highest_owners_against_each_other() {
    let a = scanned(bare_mod("a", "A"), vec![versioned_assembly("customlib", 1)]);
    let b = scanned(bare_mod("b", "B"), vec![versioned_assembly("customlib", 2)]);
    let c = scanned(bare_mod("c", "C"), vec![versioned_assembly("customlib", 3)]);
    let scanned_mods = vec![a, b, c];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = assembly_version_precedence_edges(&indices);

    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|e| e.kind == EdgeKind::AssemblyVersionPrecedence && e.before == ModId::new("c"))
    );
    let afters: HashSet<&ModId> = edges.iter().map(|e| &e.after).collect();
    assert_eq!(afters, HashSet::from([&ModId::new("a"), &ModId::new("b")]));
}

/// Two owners tied at the highest version get an edge to every lower
/// owner each, but no edge between the two tied owners themselves.
#[test]
fn assembly_version_precedence_both_tied_highest_owners_precede_the_lower_one() {
    let a = scanned(bare_mod("a", "A"), vec![versioned_assembly("customlib", 2)]);
    let b = scanned(bare_mod("b", "B"), vec![versioned_assembly("customlib", 2)]);
    let c = scanned(bare_mod("c", "C"), vec![versioned_assembly("customlib", 1)]);
    let scanned_mods = vec![a, b, c];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = assembly_version_precedence_edges(&indices);

    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| e.after == ModId::new("c")));
    let befores: HashSet<&ModId> = edges.iter().map(|e| &e.before).collect();
    assert_eq!(befores, HashSet::from([&ModId::new("a"), &ModId::new("b")]));
}

/// An owner whose copy's version metadata failed to parse (`version:
/// None`) must not participate — neither as the highest-version copy
/// nor as a lower one ordered before it. With only one *parsed*
/// version left among the two owners, there's no pair to order.
#[test]
fn assembly_version_precedence_owner_with_no_parsed_version_never_participates() {
    let no_version = scanned(
        bare_mod("no_version", "NoVersion"),
        vec![assembly("customlib", vec![])],
    );
    let versioned = scanned(
        bare_mod("versioned", "Versioned"),
        vec![versioned_assembly("customlib", 1)],
    );
    let scanned_mods = vec![no_version, versioned];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(assembly_version_precedence_edges(&indices).is_empty());
}

/// `0.0.0.0` is the CLR default `AssemblyVersion` a DLL gets when it's
/// built with no explicit `[AssemblyVersion]` attribute — not real
/// version evidence, just the compiler's zero-fill. Must be excluded
/// from consideration exactly like `version: None`, never treated as a
/// genuine "strictly lower than everything" version: on the real
/// install, `example.zonegate` bundles `0ExampleColorTool` built this way
/// while two other mods bundle a real `1.0.0.0` copy, so treating
/// `0.0.0.0` as a real version fabricated two `Declared` edges neither
/// mod's metadata actually supports.
#[test]
fn assembly_version_precedence_owner_at_clr_default_version_never_participates() {
    let zero_version = scanned(
        bare_mod("zero_version", "ZeroVersion"),
        vec![versioned_assembly("customlib", 0)],
    );
    let versioned = scanned(
        bare_mod("versioned", "Versioned"),
        vec![versioned_assembly("customlib", 1)],
    );
    let scanned_mods = vec![zero_version, versioned];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(assembly_version_precedence_edges(&indices).is_empty());
}

/// A name inferred as a shared library (shipped by
/// [`SHARED_LIBRARY_OWNER_THRESHOLD`] or more distinct owners) is excluded
/// from version-precedence edges the same way [`assembly_ref_edges`]
/// excludes it from cross-mod dependency edges: on a real install a
/// widely-adopted library would otherwise dominate the edge set,
/// over-eagerly ordering unrelated mods that merely happen to bundle
/// different builds of the same dependency.
#[test]
fn assembly_version_precedence_excludes_an_inferred_shared_library() {
    let mut scanned_mods: Vec<ScannedMod> = (0..SHARED_LIBRARY_OWNER_THRESHOLD - 2)
        .map(|i| {
            let id = format!("owner{i}");
            scanned(bare_mod(&id, &id), vec![versioned_assembly("sharedlib", 1)])
        })
        .collect();
    scanned_mods.push(scanned(
        bare_mod("old", "Old"),
        vec![versioned_assembly("sharedlib", 1)],
    ));
    scanned_mods.push(scanned(
        bare_mod("new", "New"),
        vec![versioned_assembly("sharedlib", 2)],
    ));
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    assert_eq!(
        indices.assembly_owners.get("sharedlib").map(Vec::len),
        Some(SHARED_LIBRARY_OWNER_THRESHOLD),
        "test setup must actually reach the threshold"
    );

    assert!(assembly_version_precedence_edges(&indices).is_empty());
}

/// A mod carrying declared dependencies onto another owner of the shared
/// assembly it ships — the shape [`designated_provider`](super::assembly::designated_provider)
/// needs a candidate to win.
fn depends_on(id: &str, provider_id: &str) -> Mod {
    Mod {
        declared: DeclaredOrder {
            dependencies: vec![ModDependency {
                id: ModId::new(provider_id),
                display_name: None,
            }],
            ..DeclaredOrder::default()
        },
        ..bare_mod(id, id)
    }
}

/// Regression for a small install (well under
/// [`SHARED_LIBRARY_OWNER_THRESHOLD`]) where the real designated provider
/// ships an older copy than the mods that bundle it: a strict majority of
/// the other owners declaring a dependency on it must keep it from ever
/// being subordinated by "highest version wins" — the exact bug the old
/// hand-written exclusion list existed to prevent, reopened by a bare
/// owner-count threshold on an install too small to trip it.
#[test]
fn assembly_version_precedence_never_subordinates_a_designated_provider_on_a_small_install() {
    let provider = scanned(
        bare_mod("provider", "Provider"),
        vec![versioned_assembly("sharedlib", 1)],
    );
    let bundler_a = scanned(
        depends_on("bundler_a", "provider"),
        vec![versioned_assembly("sharedlib", 2)],
    );
    let bundler_b = scanned(
        depends_on("bundler_b", "provider"),
        vec![versioned_assembly("sharedlib", 2)],
    );
    let scanned_mods = vec![provider, bundler_a, bundler_b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    assert!(
        indices.assembly_owners.get("sharedlib").map_or(0, Vec::len)
            < SHARED_LIBRARY_OWNER_THRESHOLD,
        "test setup must stay below the owner-count backstop -- provider \
         inference, not the threshold, is what this test is about"
    );

    assert!(
        assembly_version_precedence_edges(&indices).is_empty(),
        "the provider must never be ordered after a bundler on version grounds"
    );
}

/// The `assembly_ref_constraints` sibling of the regression above: a
/// referencing mod that doesn't ship its own copy gets a single-candidate
/// constraint naming the actual designated provider, never a three-way
/// any-of across every owner — most of which are just bundlers with no
/// real bearing on what the reference needs.
#[test]
fn assembly_ref_constraints_points_at_the_designated_provider_instead_of_an_any_of() {
    let provider = scanned(
        bare_mod("provider", "Provider"),
        vec![assembly("sharedlib", vec![])],
    );
    let bundler_a = scanned(
        depends_on("bundler_a", "provider"),
        vec![assembly("sharedlib", vec![])],
    );
    let bundler_b = scanned(
        depends_on("bundler_b", "provider"),
        vec![assembly("sharedlib", vec![])],
    );
    let consumer = scanned(
        bare_mod("consumer", "Consumer"),
        vec![assembly("consumer.dll", vec![("sharedlib", true)])],
    );
    let scanned_mods = vec![provider, bundler_a, bundler_b, consumer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let load_order = LoadOrder::new(scanned_mods.iter().map(|m| m.info.id.clone()).collect());

    let constraints = assembly_ref_constraints(&scanned_mods, &indices, &load_order);

    assert_eq!(constraints.len(), 1);
    let Constraint::AnyOf { candidates, .. } = &constraints[0];
    assert_eq!(candidates, &[ModId::new("provider")]);
}

/// One owner short of a strict majority: two candidates each declared by
/// exactly one of three other owners is a tie at the top, never a
/// designated provider — the ordinary, undeclared-relationship any-of
/// still fires across every owner.
#[test]
fn assembly_ref_constraints_falls_back_to_a_full_any_of_when_no_owner_has_a_clear_majority() {
    let a = scanned(bare_mod("a", "A"), vec![assembly("sharedlib", vec![])]);
    let b = scanned(bare_mod("b", "B"), vec![assembly("sharedlib", vec![])]);
    let consumer = scanned(
        bare_mod("consumer", "Consumer"),
        vec![assembly("consumer.dll", vec![("sharedlib", true)])],
    );
    let scanned_mods = vec![a, b, consumer];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let load_order = LoadOrder::new(scanned_mods.iter().map(|m| m.info.id.clone()).collect());

    let constraints = assembly_ref_constraints(&scanned_mods, &indices, &load_order);

    assert_eq!(constraints.len(), 1);
    let Constraint::AnyOf { candidates, .. } = &constraints[0];
    assert_eq!(
        candidates.len(),
        2,
        "no declared relation -- both owners remain candidates"
    );
}

/// A whole-def-injection op (`target: None`, `injected_paths` holding
/// the `"{element_tag}/{defName}"` shape) contributes its mod as an
/// owner in [`injected_def_owners`].
fn whole_def_injecting_op(def_type: &str, def_name: &str) -> PatchOp {
    PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        xpath: Some("Defs".to_string()),
        injected_paths: BTreeSet::from([format!("{def_type}/{def_name}")]),
        ..patch_op("PatchOperationAdd", None)
    }
}

#[test]
fn injected_def_owners_lists_a_mod_that_patch_injects_a_whole_def() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![whole_def_injecting_op("ThingDef", "NewThing")];
    let scanned_mods = vec![a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let owners = injected_def_owners(&scanned_mods, &active, &name_map);

    assert_eq!(
        owners.get(&("ThingDef".to_string(), "NewThing".to_string())),
        Some(&vec![ModId::new("a")])
    );
}

/// An op gated off by its own unsatisfied `MayRequire` contributes no
/// ownership.
#[test]
fn injected_def_owners_excludes_an_op_gated_off_by_unsatisfied_may_require() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        may_require: vec!["not.active".to_string()],
        // A list item is the one shape the game actually reads MayRequire
        // on; a top-level op's own attribute is unread.
        is_list_item: true,
        ..whole_def_injecting_op("ThingDef", "NewThing")
    }];
    let scanned_mods = vec![a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(injected_def_owners(&scanned_mods, &active, &name_map).is_empty());
}

/// An op nested
/// only inside a `PatchOperationFindMod`'s `<match>` branch for a mod
/// that isn't active must not count as an owner — the classic "this
/// def only exists when the other compat mod is present" shape.
/// `may_require_satisfied` alone can't see this at all; only
/// `patch_op_active`'s full gating, which needs `name_map`, can — the
/// reason this is a free function taking `name_map` rather than a field
/// on `Indices` (built before the name map exists).
#[test]
fn injected_def_owners_excludes_an_op_gated_off_by_an_unsatisfied_find_mod_branch() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_context: vec![FindModGate::AnyActive(vec!["Inactive Mod".to_string()])],
        ..whole_def_injecting_op("ThingDef", "NewThing")
    }];
    let scanned_mods = vec![a];
    let active = active_of(&scanned_mods);
    // No mod named "Inactive Mod" is active, so `name_map` never
    // resolves it — the gate stays closed.
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(injected_def_owners(&scanned_mods, &active, &name_map).is_empty());
}

/// The positive control for the fix above: the identical shape, but
/// the named mod actually is active, so the gate opens and the op
/// counts as an owner — proves the fix isn't just conservatively
/// dropping every `FindMod`-gated op.
#[test]
fn injected_def_owners_includes_an_op_gated_by_a_satisfied_find_mod_branch() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        find_mod_context: vec![FindModGate::AnyActive(vec!["Other Mod".to_string()])],
        ..whole_def_injecting_op("ThingDef", "NewThing")
    }];
    let other = scanned(bare_mod("other", "Other Mod"), vec![]);
    let scanned_mods = vec![a, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let owners = injected_def_owners(&scanned_mods, &active, &name_map);

    assert_eq!(
        owners.get(&("ThingDef".to_string(), "NewThing".to_string())),
        Some(&vec![ModId::new("a")])
    );
}

/// An element-injection op (has its own `DefTarget`) never contributes
/// — only the target-less whole-`<Defs>`-root shape does.
#[test]
fn injected_def_owners_excludes_an_element_injection_op() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        target: Some(def_name_target("ThingDef", "Human")),
        injected_paths: BTreeSet::from(["ThingDef/Human/alienRace".to_string()]),
        ..whole_def_injecting_op("ThingDef", "Human")
    }];
    let scanned_mods = vec![a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(injected_def_owners(&scanned_mods, &active, &name_map).is_empty());
}

/// A malformed (or future-mis-shaped)
/// `injected_paths` entry with more than one `/` must never be
/// silently mis-split into the wrong `DefKey` — it's dropped instead.
#[test]
fn injected_def_owners_ignores_a_malformed_multi_segment_path() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.patch_ops = vec![PatchOp {
        injected_template_names: std::collections::BTreeSet::new(),
        injected_paths: BTreeSet::from(["ThingDef/Sub/Extra".to_string()]),
        ..whole_def_injecting_op("ThingDef", "Sub")
    }];
    let scanned_mods = vec![a];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    assert!(injected_def_owners(&scanned_mods, &active, &name_map).is_empty());
}

// -- parent_name_targets ----------------------------------------------

#[test]
fn parent_name_targets_resolves_every_concrete_and_template_child() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::from([(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("a")],
    )]);
    let children_by_template: BTreeMap<(String, String), Vec<(ModId, DefKey)>> =
        BTreeMap::from([(
            ("ThingDef".to_string(), "WallBase".to_string()),
            vec![
                // A concrete def — present in `owners_by_def`.
                (
                    ModId::new("a"),
                    ("ThingDef".to_string(), "Wall".to_string()),
                ),
                // A template-only child — absent from `owners_by_def`.
                (
                    ModId::new("b"),
                    ("ThingDef".to_string(), "WallSubBase".to_string()),
                ),
            ],
        )]);

    let targets = parent_name_targets(
        r#"Defs/ThingDef[@ParentName="WallBase"]"#,
        &owners_by_def,
        &children_by_template,
    );

    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].def_name, "Wall");
    assert_eq!(targets[0].selector, Selector::DefName);
    assert_eq!(targets[1].def_name, "WallSubBase");
    assert_eq!(targets[1].selector, Selector::NameAttr);
}

#[test]
fn parent_name_targets_shares_the_sub_path_across_every_child() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::from([(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("a")],
    )]);
    let children_by_template: BTreeMap<(String, String), Vec<(ModId, DefKey)>> =
        BTreeMap::from([(
            ("ThingDef".to_string(), "WallBase".to_string()),
            vec![(
                ModId::new("a"),
                ("ThingDef".to_string(), "Wall".to_string()),
            )],
        )]);

    let targets = parent_name_targets(
        r#"Defs/ThingDef[@ParentName="WallBase"]/statBases"#,
        &owners_by_def,
        &children_by_template,
    );

    assert_eq!(targets[0].sub_path.as_deref(), Some("statBases"));
}

#[test]
fn parent_name_targets_is_empty_when_no_child_registers_that_parent() {
    let targets = parent_name_targets(
        r#"Defs/ThingDef[@ParentName="Nobody"]"#,
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(targets.is_empty());
}

#[test]
fn parent_name_targets_is_empty_for_an_ordinary_def_name_head() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::new();
    let children_by_template: BTreeMap<(String, String), Vec<(ModId, DefKey)>> = BTreeMap::new();
    let targets = parent_name_targets(
        r#"Defs/ThingDef[defName="Wall"]"#,
        &owners_by_def,
        &children_by_template,
    );
    assert!(targets.is_empty());
}

// -- child_value_targets ----------------------------------------------

fn child_value_key(def_type: &str, def_name: &str, path: &str, value: &str) -> u64 {
    crate::domain::hash_node_path(&format!("{def_type}/{def_name}/{path}={value}"))
}

#[test]
fn child_value_targets_resolves_the_real_pawn_badge_shape() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::from([
        (
            (
                "ExampleRace.ThingDef_ExampleRace".to_string(),
                "Human".to_string(),
            ),
            vec![ModId::new("harmod")],
        ),
        (
            (
                "ExampleRace.ThingDef_ExampleRace".to_string(),
                "Muffalo".to_string(),
            ),
            vec![ModId::new("harmod")],
        ),
    ]);
    let child_value_hashes_by_mod = BTreeMap::from([(
        ModId::new("harmod"),
        HashSet::from([
            child_value_key(
                "ExampleRace.ThingDef_ExampleRace",
                "Human",
                "race/intelligence",
                "Humanlike",
            ),
            child_value_key(
                "ExampleRace.ThingDef_ExampleRace",
                "Muffalo",
                "race/intelligence",
                "Animal",
            ),
        ]),
    )]);

    let targets = child_value_targets(
        r#"Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps"#,
        &owners_by_def,
        &child_value_hashes_by_mod,
    );

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].def_name, "Human");
    assert_eq!(targets[0].selector, Selector::DefName);
    assert_eq!(targets[0].sub_path.as_deref(), Some("comps"));
}

/// Only the *winning* (last-loaded) owner's own content counts — a
/// losing owner's copy having the value must never resolve the def
/// (the real engine never keeps a losing owner's XML loaded once
/// patches run at all).
#[test]
fn child_value_targets_only_trusts_the_winning_owners_content() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::from([(
        ("ThingDef".to_string(), "Race".to_string()),
        vec![ModId::new("loser"), ModId::new("winner")],
    )]);
    let child_value_hashes_by_mod = BTreeMap::from([
        (
            ModId::new("loser"),
            HashSet::from([child_value_key(
                "ThingDef",
                "Race",
                "race/intelligence",
                "Humanlike",
            )]),
        ),
        (
            ModId::new("winner"),
            HashSet::from([child_value_key(
                "ThingDef",
                "Race",
                "race/intelligence",
                "Animal",
            )]),
        ),
    ]);

    let targets = child_value_targets(
        r#"Defs/ThingDef[race/intelligence="Humanlike"]"#,
        &owners_by_def,
        &child_value_hashes_by_mod,
    );

    assert!(
        targets.is_empty(),
        "the winning owner's own content says 'Animal', not 'Humanlike'"
    );
}

#[test]
fn child_value_targets_is_empty_for_an_ordinary_def_name_head() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::new();
    let child_value_hashes_by_mod: BTreeMap<ModId, HashSet<u64>> = BTreeMap::new();
    let targets = child_value_targets(
        r#"Defs/ThingDef[defName="Wall"]"#,
        &owners_by_def,
        &child_value_hashes_by_mod,
    );
    assert!(targets.is_empty());
}

#[test]
fn child_value_targets_is_empty_when_no_owner_of_that_type_matches() {
    let owners_by_def: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::from([(
        ("ThingDef".to_string(), "Chair".to_string()),
        vec![ModId::new("a")],
    )]);
    let child_value_hashes_by_mod = BTreeMap::from([(ModId::new("a"), HashSet::new())]);

    let targets = child_value_targets(
        r#"Defs/ThingDef[race/intelligence="Humanlike"]"#,
        &owners_by_def,
        &child_value_hashes_by_mod,
    );
    assert!(targets.is_empty());
}

// -- retexture_after_owner_edges -------------------------------------

fn def_entry(def_type: &str, def_name: &str) -> crate::domain::DefEntry {
    crate::domain::DefEntry {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
        may_require: Vec::new(),
        may_require_any_of: Vec::new(),
        parent_name: None,
        locator: XmlLocator::for_test(),
    }
}

/// The core rule: a texture-only mod sharing a path with exactly one
/// content owner must load after it.
#[test]
fn retexture_after_owner_edge_when_a_texture_only_mod_overrides_a_content_owner() {
    let mut texture_only = scanned(bare_mod("retex", "Retex"), vec![]);
    texture_only.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    let mut content = scanned(bare_mod("content", "Content"), vec![]);
    content.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    content.defs = vec![def_entry("ThingDef", "Wall")];
    let scanned_mods = vec![texture_only, content];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = retexture_after_owner_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("retex"));
    assert_eq!(edges[0].before, ModId::new("content"));
    assert_eq!(edges[0].kind, EdgeKind::RetextureAfterOwner);
    assert_eq!(edges[0].subject.as_deref(), Some("things/wall"));
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// Two content owners sharing a path: the ledger's own
/// `TextureOverride` conflict stands, this producer stays silent —
/// it can't tell which content owner should win.
#[test]
fn retexture_after_owner_edges_emit_nothing_for_two_content_owners() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    a.defs = vec![def_entry("ThingDef", "A")];
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    b.defs = vec![def_entry("ThingDef", "B")];
    let scanned_mods = vec![a, b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(retexture_after_owner_edges(&scanned_mods, &indices).is_empty());
}

/// Two texture-only owners sharing a path: same silence, same reason
/// — no single texture-only mod can be singled out as "the" override.
#[test]
fn retexture_after_owner_edges_emit_nothing_for_two_texture_only_owners() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    let scanned_mods = vec![a, b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(retexture_after_owner_edges(&scanned_mods, &indices).is_empty());
}

/// Several shared paths between the same (texture-only, content) pair
/// collapse to one edge, the count legible in `detail` — not silently
/// dropped after the first the way most of this module's producers
/// let `push_edge_once` behave.
#[test]
fn retexture_after_owner_edges_collapse_several_shared_paths_into_one_edge_with_a_count() {
    let mut texture_only = scanned(bare_mod("retex", "Retex"), vec![]);
    texture_only.textures = BTreeMap::from([
        ("things/wall".to_string(), 0),
        ("things/door".to_string(), 0),
    ]);
    let mut content = scanned(bare_mod("content", "Content"), vec![]);
    content.textures = BTreeMap::from([
        ("things/wall".to_string(), 0),
        ("things/door".to_string(), 0),
    ]);
    content.defs = vec![def_entry("ThingDef", "Wall")];
    let scanned_mods = vec![texture_only, content];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = retexture_after_owner_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert!(edges[0].detail.contains('2'), "detail: {}", edges[0].detail);
    assert_eq!(
        edges[0].subject.as_deref(),
        Some("things/door"),
        "alphabetically first of the two shared paths"
    );
}

/// A content owner whose *only* content is a mutating patch op (no
/// `defs`/`templates`/`assemblies` at all) must still count as content,
/// not texture-only — this is the only test that would catch the
/// `!patch_ops.iter().any(is_mutating)` clause being weakened to
/// `patch_ops.is_empty()`.
#[test]
fn retexture_after_owner_edge_still_fires_when_the_content_owner_is_patch_only() {
    let mut texture_only = scanned(bare_mod("retex", "Retex"), vec![]);
    texture_only.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    let mut content = scanned(bare_mod("content", "Content"), vec![]);
    content.textures = BTreeMap::from([("things/wall".to_string(), 0)]);
    content.patch_ops = vec![patch_op(
        "PatchOperationAdd",
        Some(def_name_target("ThingDef", "Wall")),
    )];
    let scanned_mods = vec![texture_only, content];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = retexture_after_owner_edges(&scanned_mods, &indices);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("retex"));
    assert_eq!(edges[0].before, ModId::new("content"));
}

// -- def_override_after_origin_edges ---------------------------------

/// Origin rule 1: a candidate owner's own copy of the def carries a
/// `ParentName` naming a `Name`-attributed template that same owner
/// also ships.
#[test]
fn def_override_after_origin_edge_via_parent_template_ownership() {
    use crate::domain::TemplateEntry;
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![crate::domain::DefEntry {
        parent_name: Some("WallBase".to_string()),
        ..def_entry("ThingDef", "Wall")
    }];
    origin.templates = vec![TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: "ThingDef".to_string(),
        name: "WallBase".to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![def_entry("ThingDef", "Wall")];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = def_override_after_origin_edges(&scanned_mods, &indices, &[]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("other"));
    assert_eq!(edges[0].before, ModId::new("origin"));
    assert_eq!(edges[0].kind, EdgeKind::DefOverrideAfterOrigin);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/Wall"));
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// `parent_name_by_owner_def`
/// is built over every one of a mod's own `defs`, deliberately not
/// `may_require_satisfied`-filtered like `indices.def_owners` is —
/// so a mod shipping two `MayRequire`-gated variant copies of the
/// same def (a `None`-parent one listed first, a `Some`-parent one
/// second) must not have rule 1 read the gated-off copy's missing
/// `ParentName` just because it happened to come first.
#[test]
fn def_override_after_origin_edges_prefers_a_some_parent_name_over_a_none_on_collision() {
    use crate::domain::TemplateEntry;
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![
        crate::domain::DefEntry {
            may_require: vec!["some.other.gate".to_string()],
            ..def_entry("ThingDef", "Wall")
        },
        crate::domain::DefEntry {
            parent_name: Some("WallBase".to_string()),
            ..def_entry("ThingDef", "Wall")
        },
    ];
    origin.templates = vec![TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: "ThingDef".to_string(),
        name: "WallBase".to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![def_entry("ThingDef", "Wall")];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = def_override_after_origin_edges(&scanned_mods, &indices, &[]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("other"));
    assert_eq!(edges[0].before, ModId::new("origin"));
}

/// Origin rule 2: a candidate owner owns strictly more defs (any
/// `def_type`) sharing the overridden def's own name prefix than the
/// def's other owner(s).
#[test]
fn def_override_after_origin_edge_via_def_name_family_prefix() {
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
        def_entry("ThingDef", "XAM_Chair"),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![def_entry("ThingDef", "XAM_Wall")];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = def_override_after_origin_edges(&scanned_mods, &indices, &[]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("other"));
    assert_eq!(edges[0].before, ModId::new("origin"));
    assert!(
        edges[0].detail.contains("XAM_"),
        "detail: {}",
        edges[0].detail
    );
}

/// Modeled on a real `FurDef/XV_BarkSkin` shape: the prefix rule must count across **every**
/// def type, not just the overridden def's own — a namespace prefix
/// is a cross-type authorship signal (one mod stamps `XV_` on its
/// `ThingDef`s and its `FurDef`s alike). `origin` here owns far more
/// `XV_`-prefixed defs overall, but *fewer* `FurDef`-typed ones than
/// `other` — a `def_type`-scoped rule would pick `other` as the origin
/// here, inverting the real `examplevr.mossveil` /
/// `example.apparelscale.extended` case.
#[test]
fn def_override_after_origin_edge_via_def_name_family_prefix_counts_across_every_def_type() {
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![
        def_entry("FurDef", "XV_BarkSkin"),
        def_entry("ThingDef", "XV_Gene1"),
        def_entry("ThingDef", "XV_Gene2"),
        def_entry("ThingDef", "XV_Gene3"),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![
        def_entry("FurDef", "XV_BarkSkin"),
        def_entry("FurDef", "XV_OtherFur1"),
    ];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    let edges = def_override_after_origin_edges(&scanned_mods, &indices, &[]);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("other"));
    assert_eq!(
        edges[0].before,
        ModId::new("origin"),
        "origin owns 4 XV_-prefixed defs overall (vs. other's 2), even though \
             other owns more of the narrower FurDef-only count (2 vs. 1)"
    );
}

/// Ambiguous: both owners own the identical count of prefix-sharing
/// defs, so neither strictly beats the other — no edge.
#[test]
fn def_override_after_origin_edges_emit_nothing_when_prefix_counts_tie() {
    let mut a = scanned(bare_mod("a", "A"), vec![]);
    a.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
    ];
    let mut b = scanned(bare_mod("b", "B"), vec![]);
    b.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Chair"),
    ];
    let scanned_mods = vec![a, b];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(def_override_after_origin_edges(&scanned_mods, &indices, &[]).is_empty());
}

/// A `Declared`-strength edge already ordering the two owners (in
/// either direction) suppresses inference for the whole def entirely
/// — even though `origin` would otherwise qualify via the prefix
/// rule, that order is already known, not inferred.
#[test]
fn def_override_after_origin_edges_skip_when_a_declared_edge_already_orders_the_owners() {
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![def_entry("ThingDef", "XAM_Wall")];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let existing_edges = vec![Edge {
        after: ModId::new("other"),
        before: ModId::new("origin"),
        kind: EdgeKind::LoadAfter,
        detail: "other declares loadAfter origin".to_string(),
        load_time: true,
        subject: None,
    }];

    assert!(def_override_after_origin_edges(&scanned_mods, &indices, &existing_edges).is_empty());
}

/// Real shape: a mod's `MapGen_`-prefixed `ThingSetMakerDef`s could
/// order Core after that mod. A generic-word prefix can make a mod's own
/// count exceed vanilla's, but RimWorld can never load Core/DLC after
/// a mod — vanilla must never appear as the losing `other` owner,
/// even when it would otherwise lose the prefix count fair and
/// square. Fails without the vanilla filter: `mod_a` owns 3 `XAM_`
/// defs to Core's 1, so `mod_a` is (correctly) the origin — and no
/// `after: core, before: mod_a` edge may be emitted for the loser.
#[test]
fn def_override_after_origin_edges_never_orders_a_vanilla_owner_after_a_mod() {
    let mut mod_a = scanned(bare_mod("mod_a", "ModA"), vec![]);
    mod_a.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
        def_entry("ThingDef", "XAM_Chair"),
    ];
    let mut core = scanned(bare_mod("ludeon.rimworld", "Core"), vec![]);
    core.info.source = Source::Core;
    core.defs = vec![def_entry("ThingDef", "XAM_Wall")];
    let scanned_mods = vec![mod_a, core];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(
        def_override_after_origin_edges(&scanned_mods, &indices, &[]).is_empty(),
        "must never order Core after a mod, even when the mod's own \
             prefix count legitimately exceeds Core's"
    );
}

/// A def where *one*
/// owner qualifies via the ParentName-template rule and a *different*
/// owner independently qualifies via the prefix rule is ambiguous
/// over the union of both rules, not resolved by either alone.
#[test]
fn def_override_after_origin_edges_emit_nothing_when_different_owners_qualify_via_different_rules()
{
    use crate::domain::TemplateEntry;
    let mut via_template = scanned(bare_mod("via_template", "ViaTemplate"), vec![]);
    via_template.defs = vec![crate::domain::DefEntry {
        parent_name: Some("WallBase".to_string()),
        ..def_entry("ThingDef", "XAM_Wall")
    }];
    via_template.templates = vec![TemplateEntry {
        graphic_class: None,
        may_require: Vec::new(),
        def_type: "ThingDef".to_string(),
        name: "WallBase".to_string(),
        parent_name: None,
        is_abstract: true,
        locator: XmlLocator::for_test(),
    }];
    let mut via_prefix = scanned(bare_mod("via_prefix", "ViaPrefix"), vec![]);
    via_prefix.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
        def_entry("ThingDef", "XAM_Chair"),
    ];
    let scanned_mods = vec![via_template, via_prefix];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());

    assert!(def_override_after_origin_edges(&scanned_mods, &indices, &[]).is_empty());
}

/// The "no Declared/Hard edge between the owners"
/// gate must also fire on a `Hard`-strength edge, not just
/// `Declared` — pins the other half of the `matches!(..., Declared |
/// Hard)` filter (the sibling test above only exercises `LoadAfter`,
/// itself `Declared`).
#[test]
fn def_override_after_origin_edges_skip_when_a_hard_edge_already_orders_the_owners() {
    let mut origin = scanned(bare_mod("origin", "Origin"), vec![]);
    origin.defs = vec![
        def_entry("ThingDef", "XAM_Wall"),
        def_entry("ThingDef", "XAM_Table"),
    ];
    let mut other = scanned(bare_mod("other", "Other"), vec![]);
    other.defs = vec![def_entry("ThingDef", "XAM_Wall")];
    let scanned_mods = vec![origin, other];
    let active = active_of(&scanned_mods);
    let indices = Indices::build(&scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let existing_edges = vec![Edge {
        after: ModId::new("other"),
        before: ModId::new("origin"),
        kind: EdgeKind::PatchInjectedNode,
        detail: "other patch-selects a node origin injects".to_string(),
        load_time: true,
        subject: Some("ThingDef/Something".to_string()),
    }];

    assert!(def_override_after_origin_edges(&scanned_mods, &indices, &existing_edges).is_empty());
}

/// [`def_name_family_prefix`]'s three documented rejections
/// — no `_` at all, a leading `_` (prefix too short), and a
/// single-character-plus-underscore prefix (also too short).
#[test]
fn def_name_family_prefix_rejects_names_with_no_qualifying_prefix() {
    assert_eq!(def_name_family_prefix("Wall"), None, "no underscore at all");
    assert_eq!(
        def_name_family_prefix("_Wall"),
        None,
        "leading underscore: prefix is just \"_\""
    );
    assert_eq!(
        def_name_family_prefix("A_B"),
        None,
        "\"A_\" is only 2 characters"
    );
    assert_eq!(
        def_name_family_prefix("ZZ_Wall"),
        Some("ZZ_"),
        "3-character prefix is the minimum that qualifies"
    );
}

// -- manifest_order_edges ---------------------------------------------

/// A Manifest.xml entry naming another mod by its **display name**
/// (not its packageId) still resolves, the same way `find_mod_edges`
/// resolves a `PatchOperationFindMod` name.
#[test]
fn manifest_order_edge_resolves_a_display_name_entry() {
    use crate::domain::ManifestOrder;
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = ManifestOrder {
        load_after: vec!["Other Mod".to_string()],
        load_before: Vec::new(),
        dependencies: Vec::new(),
    };
    let other = scanned(bare_mod("other", "Other Mod"), vec![]);
    let scanned_mods = vec![m, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty());
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("m"));
    assert_eq!(edges[0].before, ModId::new("other"));
    assert_eq!(edges[0].kind, EdgeKind::LoadAfter);
    assert!(edges[0].detail.contains("Manifest.xml"));
}

/// An entry that resolves to no active mod, by either name, is a
/// warning — never a silently-dropped edge.
#[test]
fn manifest_order_edge_reports_a_warning_for_an_unresolvable_entry() {
    use crate::domain::ManifestOrder;
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = ManifestOrder {
        load_after: vec!["Nonexistent Mod".to_string()],
        load_before: Vec::new(),
        dependencies: Vec::new(),
    };
    let scanned_mods = vec![m];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(edges.is_empty());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Nonexistent Mod"));
    assert_eq!(warnings[0].mod_id, Some(ModId::new("m")));
}

/// A rule present in both `About.xml` (already in `existing_edges`,
/// standing in for `declared_edges`'s own output) and `Manifest.xml`
/// dedupes to the one edge — the Manifest.xml duplicate never adds a
/// second one.
#[test]
fn manifest_order_edge_dedupes_against_an_existing_about_xml_rule() {
    use crate::domain::ManifestOrder;
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = ManifestOrder {
        load_after: vec!["other".to_string()],
        load_before: Vec::new(),
        dependencies: Vec::new(),
    };
    let other = scanned(bare_mod("other", "Other"), vec![]);
    let scanned_mods = vec![m, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);
    let existing_edges = vec![Edge {
        after: ModId::new("m"),
        before: ModId::new("other"),
        kind: EdgeKind::LoadAfter,
        detail: "m declares loadAfter other".to_string(),
        load_time: true,
        subject: None,
    }];

    let (edges, warnings) =
        manifest_order_edges(&scanned_mods, &active, &name_map, &existing_edges);

    assert!(warnings.is_empty());
    assert!(
        edges.is_empty(),
        "About.xml's own edge already covers this pair"
    );
}

/// A community-manager-style version-bound suffix
/// (`"ExampleFramework >= 5.5.0"`) must not stop the entry from resolving
/// against the plain display name.
#[test]
fn manifest_order_edge_resolves_a_version_bounded_entry() {
    use crate::domain::ManifestOrder;
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = ManifestOrder {
        load_after: Vec::new(),
        load_before: Vec::new(),
        dependencies: vec!["ExampleFramework >= 5.5.0".to_string()],
    };
    let example = scanned(bare_mod("example.framework", "Example Framework"), vec![]);
    let scanned_mods = vec![m, example];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("example.framework"));
    assert_eq!(edges[0].kind, EdgeKind::ModDependency);
    assert!(
        edges[0].detail.contains("ExampleFramework >= 5.5.0"),
        "detail keeps the original raw text: {}",
        edges[0].detail
    );
}

/// An identical raw entry (the same unresolvable name appearing in
/// both `loadAfter` and `dependencies`) warns once per mod, not once
/// per list it appears in.
#[test]
fn manifest_order_edge_dedupes_a_repeated_unresolvable_entry_across_lists() {
    use crate::domain::ManifestOrder;
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = ManifestOrder {
        load_after: vec!["ExampleRaceFramework".to_string()],
        load_before: Vec::new(),
        dependencies: vec!["ExampleRaceFramework".to_string()],
    };
    let scanned_mods = vec![m];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(edges.is_empty());
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
}

/// A mod whose on-disk folder is `folder` rather than its packageId —
/// the shape a Workshop subscription has (`<workshop>/<id>/`).
fn mod_in_folder(id: &str, name: &str, folder: &str) -> Mod {
    Mod {
        path: PathBuf::from(folder),
        ..bare_mod(id, name)
    }
}

fn manifest_load_after(entries: &[&str]) -> crate::domain::ManifestOrder {
    crate::domain::ManifestOrder {
        load_after: entries.iter().map(|e| (*e).to_string()).collect(),
        load_before: Vec::new(),
        dependencies: Vec::new(),
    }
}

/// `example.genetics.xenoforms`'s
/// real `ExampleExoGenes` entry names the active "Example Exo Genes"
/// with the spaces squeezed out — no exact match either way, but a
/// unique normalized one.
#[test]
fn manifest_order_edge_resolves_a_normalized_display_name_entry() {
    let mut m = scanned(bare_mod("example.genetics.xenoforms", "X"), vec![]);
    m.manifest_order = manifest_load_after(&["ExampleExoGenes"]);
    let genes = scanned(
        bare_mod("example.genetics.exogenes", "Example Exo Genes"),
        vec![],
    );
    let scanned_mods = vec![m, genes];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("example.genetics.exogenes"));
}

/// Both fallback-worthy features of one real entry at once: a version
/// bound to strip, then a normalized match — and a sibling mod
/// ("Example Compendium Ideology") whose own normalized key must not be
/// confused with it.
#[test]
fn manifest_order_edge_resolves_a_version_bounded_normalized_entry() {
    let mut m = scanned(bare_mod("example.dormant.compendium", "P"), vec![]);
    m.manifest_order = manifest_load_after(&["ExampleCompendium >= 1.5.1.7"]);
    let compendium = scanned(bare_mod("example.compendium", "Example Compendium"), vec![]);
    let ideology = scanned(
        bare_mod("example.compendium.ideology", "Example Compendium Ideology"),
        vec![],
    );
    let scanned_mods = vec![m, compendium, ideology];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("example.compendium"));
}

/// Two active mods sharing a normalized key resolve to *neither*: a
/// wrong pick here would fabricate an author declaration nobody made.
#[test]
fn manifest_order_edge_leaves_an_ambiguous_normalized_entry_unresolved() {
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = manifest_load_after(&["FooBar"]);
    let spaced = scanned(bare_mod("a.foo", "Foo Bar"), vec![]);
    let hyphenated = scanned(bare_mod("b.foo", "foo-bar"), vec![]);
    let scanned_mods = vec![m, spaced, hyphenated];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    let message = &warnings[0].message;
    assert!(
        message.contains("matches more than one active mod")
            && message.contains("a.foo")
            && message.contains("b.foo"),
        "the warning must name both candidates: {message}"
    );
}

/// A bare Workshop id resolves through the mod's own content-folder name.
#[test]
fn manifest_order_edge_resolves_a_workshop_id_entry_by_folder_name() {
    let mut m = scanned(bare_mod("example.treechop", "T"), vec![]);
    m.manifest_order = manifest_load_after(&["3000000013"]);
    let subscribed = scanned(
        mod_in_folder("some.author.mod", "Some Mod", "3000000013"),
        vec![],
    );
    let scanned_mods = vec![m, subscribed];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("some.author.mod"));
}

/// The same entry with no matching folder among the active mods stays
/// unresolved — the numeric shape alone never resolves anything.
#[test]
fn manifest_order_edge_warns_for_a_workshop_id_no_active_mod_ships() {
    let mut m = scanned(bare_mod("example.treechop", "T"), vec![]);
    m.manifest_order = manifest_load_after(&["3000000014"]);
    let other = scanned(mod_in_folder("some.author.mod", "Some Mod", "111"), vec![]);
    let scanned_mods = vec![m, other];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(warnings[0].message.contains("3000000014"));
}

/// A mod installed both
/// locally and from the Workshop scans as two `ScannedMod`s sharing
/// one display name and one content-folder name
/// (`tests/steam_suffix_scan.rs`). Both copies index under the one
/// id `ActiveMods::resolve` gives their shared base, so the entry
/// resolves — reporting an ambiguity about what is a single mod
/// would be the worst of both worlds.
/// The entry is deliberately `"DupMod"`, not the exact `"Dup Mod"`:
/// an exact display-name hit would never reach the fallback index
/// this is about.
#[test]
fn manifest_order_edge_resolves_a_normalized_name_both_copies_of_one_mod_claim() {
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = manifest_load_after(&["DupMod"]);
    let local = scanned(mod_in_folder("dup.mod", "Dup Mod", "Mods/DupMod"), vec![]);
    let workshop = scanned(mod_in_folder("dup.mod_steam", "Dup Mod", "111111"), vec![]);
    let scanned_mods = vec![m, local, workshop];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].before, ModId::new("dup.mod"));
}

/// Same rule through the other fallback index: both copies of one mod
/// sitting in the same numeric folder is one Workshop id, not two.
#[test]
fn manifest_order_edge_resolves_a_workshop_folder_both_copies_of_one_mod_claim() {
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = manifest_load_after(&["111111"]);
    let local = scanned(mod_in_folder("dup.mod", "Dup Mod", "111111"), vec![]);
    let workshop = scanned(mod_in_folder("dup.mod_steam", "Dup Mod", "111111"), vec![]);
    let scanned_mods = vec![m, local, workshop];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].before, ModId::new("dup.mod"));
}

/// The same duplicate naming *itself*: `dup.mod_steam`'s entry
/// resolves to the base copy `dup.mod`, which is the same mod, so it
/// must be skipped like any other self-reference rather than emitted
/// as a `dup.mod_steam -> dup.mod` edge.
#[test]
fn manifest_order_edge_skips_a_self_reference_across_the_steam_suffix() {
    let mut workshop = scanned(mod_in_folder("dup.mod_steam", "Dup Mod", "111111"), vec![]);
    workshop.manifest_order = manifest_load_after(&["dup.mod"]);
    let local = scanned(mod_in_folder("dup.mod", "Dup Mod", "Mods/DupMod"), vec![]);
    let scanned_mods = vec![workshop, local];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert!(edges.is_empty(), "edges: {edges:?}");
}

/// An entry (and a mod name) with no alphanumeric character at all
/// normalizes to the empty string, which must never become a key
/// everything punctuation-only silently matches.
#[test]
fn manifest_order_edge_never_resolves_through_an_empty_normalized_name() {
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = manifest_load_after(&["???"]);
    let punctuation = scanned(bare_mod("p.mod", "!!!"), vec![]);
    let scanned_mods = vec![m, punctuation];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(warnings[0].message.contains("???"));
}

/// The fallbacks are strictly a *last* resort: an exact display-name
/// match wins over a normalized one pointing at a different mod (here
/// the normalized key is even ambiguous, which would otherwise warn).
#[test]
fn manifest_order_edge_prefers_an_exact_display_name_over_a_normalized_one() {
    let mut m = scanned(bare_mod("m", "M"), vec![]);
    m.manifest_order = manifest_load_after(&["Foo Bar"]);
    let exact = scanned(bare_mod("a.foo", "Foo Bar"), vec![]);
    let squeezed = scanned(bare_mod("b.foo", "FooBar"), vec![]);
    let scanned_mods = vec![m, exact, squeezed];
    let active = active_of(&scanned_mods);
    let (name_map, _) = build_name_map(&scanned_mods);

    let (edges, warnings) = manifest_order_edges(&scanned_mods, &active, &name_map, &[]);

    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].before, ModId::new("a.foo"));
}

// `replace_discards_addition` — a later `Replace` discarding an
// earlier active mod's own addition.

fn digest(entries: &[(&str, u64)]) -> ValueDigest {
    ValueDigest {
        entries: entries
            .iter()
            .map(|(path, hash)| (path.to_string(), *hash))
            .collect(),
        truncated: false,
    }
}

fn replace_discards_addition_of(
    scanned_mods: &[ScannedMod],
) -> (Vec<Edge>, Vec<crate::domain::Conflict>) {
    let active = active_of(scanned_mods);
    let indices = Indices::build(scanned_mods, &HashSet::new(), &active, &BTreeSet::new());
    let (name_map, _) = build_name_map(scanned_mods);
    replace_discards_addition(scanned_mods, &indices, &active, &name_map)
}

/// The same-node case: `B` replaces `Wall/comps`, keeping the node's own
/// tag; `A` adds distinct content to the identical `comps` sub_path. `A`
/// must load after `B`.
#[test]
fn replace_discards_addition_same_node_emits_adder_after_replacer() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].after, ModId::new("adder"));
    assert_eq!(edges[0].before, ModId::new("replacer"));
    assert_eq!(edges[0].kind, EdgeKind::ReplaceDiscardsAddition);
    assert_eq!(edges[0].subject.as_deref(), Some("ThingDef/Wall/comps"));
    assert_eq!(edges[0].strength(), crate::domain::EdgeStrength::Inferred);
}

/// The ancestor case: `B` replaces the def root (`P` has no `sub_path`),
/// `A` adds at `root/options`, strictly below `P`.
#[test]
fn replace_discards_addition_ancestor_case_emits() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["root".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/root"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 7)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/root/options"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::ReplaceDiscardsAddition);
}

/// A replacement value that renames the node (its own top-level tag
/// doesn't match `P`'s own last step) is excluded outright — no edge,
/// no finding.
#[test]
fn replace_discards_addition_renaming_value_does_not_emit() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["renamed".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// Runs the real extractor over one mod's patch XML, so a fixture's
/// `value_digest` / `value_root_names` can never drift from what
/// `extract::patches` really produces.
fn mod_with_patch_xml(id: &str, patch_xml: &str) -> ScannedMod {
    use crate::extract::patches;
    use std::sync::Arc;

    let file = Arc::from(PathBuf::from("test.xml"));
    let mut patching = scanned(bare_mod(id, id), vec![]);
    patching.patch_ops = patches::walk(patch_xml.as_bytes(), &file).unwrap();
    patching
}

fn replace_op_xml(xpath: &str, value: &str) -> String {
    format!(
        r#"<Patch><Operation Class="PatchOperationReplace"><xpath>{xpath}</xpath><value>{value}</value></Operation></Patch>"#
    )
}

fn add_op_xml(xpath: &str, value: &str) -> String {
    format!(
        r#"<Patch><Operation Class="PatchOperationAdd"><xpath>{xpath}</xpath><value>{value}</value></Operation></Patch>"#
    )
}

const GRAPHICS_XPATH: &str = r#"Defs/ThingDef[defName="Widget"]/graphics"#;

/// `A`'s own added content already appears in `B`'s own replacement at
/// the matching relative path — loading `A` after `B` would add it
/// twice, so no edge. Both ops come from the real extractor: the
/// replacer's digest keys carry the replaced node's own tag, the adder's
/// don't, and the comparison has to bridge exactly that.
#[test]
fn replace_discards_addition_duplicate_content_does_not_emit() {
    let replacer = mod_with_patch_xml(
        "replacer",
        &replace_op_xml(
            GRAPHICS_XPATH,
            "<graphics><li><texPath>Example/A</texPath></li></graphics>",
        ),
    );
    let adder = mod_with_patch_xml(
        "adder",
        &add_op_xml(GRAPHICS_XPATH, "<li><texPath>Example/A</texPath></li>"),
    );

    let (edges, conflicts) = replace_discards_addition_of(&[replacer, adder]);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// The negative that keeps the rule alive: same node, but the adder's
/// content differs from the replacement's.
#[test]
fn replace_discards_addition_distinct_content_still_emits() {
    let replacer = mod_with_patch_xml(
        "replacer",
        &replace_op_xml(
            GRAPHICS_XPATH,
            "<graphics><li><texPath>Example/A</texPath></li></graphics>",
        ),
    );
    let adder = mod_with_patch_xml(
        "adder",
        &add_op_xml(GRAPHICS_XPATH, "<li><texPath>Example/B</texPath></li>"),
    );

    let (edges, _conflicts) = replace_discards_addition_of(&[replacer, adder]);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
    assert_eq!(edges[0].kind, EdgeKind::ReplaceDiscardsAddition);
}

/// One of the adder's two items is missing from the replacement, so part
/// of its content is still lost — the edge stays.
#[test]
fn replace_discards_addition_partial_duplicate_still_emits() {
    let replacer = mod_with_patch_xml(
        "replacer",
        &replace_op_xml(
            GRAPHICS_XPATH,
            "<graphics><li><texPath>Example/A</texPath></li></graphics>",
        ),
    );
    let adder = mod_with_patch_xml(
        "adder",
        &add_op_xml(
            GRAPHICS_XPATH,
            "<li><texPath>Example/A</texPath></li><li><texPath>Example/B</texPath></li>",
        ),
    );

    let (edges, _conflicts) = replace_discards_addition_of(&[replacer, adder]);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
}

/// The ancestor case: the replacer rewrites `root`, the adder adds below
/// it at `root/options`; the replacement already holds that item.
#[test]
fn replace_discards_addition_ancestor_duplicate_does_not_emit() {
    let replacer = mod_with_patch_xml(
        "replacer",
        &replace_op_xml(
            r#"Defs/ThingDef[defName="Widget"]/root"#,
            "<root><options><li>Example_Loot</li></options></root>",
        ),
    );
    let adder = mod_with_patch_xml(
        "adder",
        &add_op_xml(
            r#"Defs/ThingDef[defName="Widget"]/root/options"#,
            "<li>Example_Loot</li>",
        ),
    );

    let (edges, conflicts) = replace_discards_addition_of(&[replacer, adder]);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// The ancestor case with different content stays an edge.
#[test]
fn replace_discards_addition_ancestor_distinct_content_still_emits() {
    let replacer = mod_with_patch_xml(
        "replacer",
        &replace_op_xml(
            r#"Defs/ThingDef[defName="Widget"]/root"#,
            "<root><options><li>Example_Loot</li></options></root>",
        ),
    );
    let adder = mod_with_patch_xml(
        "adder",
        &add_op_xml(
            r#"Defs/ThingDef[defName="Widget"]/root/options"#,
            "<li>Example_Other</li>",
        ),
    );

    let (edges, _conflicts) = replace_discards_addition_of(&[replacer, adder]);

    assert_eq!(edges.len(), 1, "edges: {edges:?}");
}

/// A truncated digest on either side can't prove a duplicate, so the
/// edge is kept (the safe, content-leaning direction) even though the
/// entries look identical.
#[test]
fn replace_discards_addition_truncated_digest_still_emits() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest {
            entries: BTreeSet::from([("li".to_string(), 1)]),
            truncated: true,
        }),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, _conflicts) = replace_discards_addition_of(&scanned_mods);

    assert_eq!(edges.len(), 1);
}

/// Owner policy: the replacer's own `About.xml` already declares it
/// loads after the adder — a deliberate override. No edge, only the
/// `DiscardedAddition` finding.
#[test]
fn replace_discards_addition_declared_override_emits_only_the_finding() {
    let mut replacer_info = bare_mod("replacer", "Replacer");
    replacer_info.declared = DeclaredOrder {
        load_after: vec![ModId::new("adder")],
        ..DeclaredOrder::default()
    };
    let mut replacer = scanned(replacer_info, vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert_eq!(conflicts.len(), 1);
    match &conflicts[0] {
        Conflict::DiscardedAddition(c) => {
            assert_eq!(c.replacer, ModId::new("replacer"));
            assert_eq!(c.adder, ModId::new("adder"));
            assert_eq!(c.path, "ThingDef/Wall/comps");
        }
        other => panic!("expected DiscardedAddition, got {other:?}"),
    }
}

/// A genuinely conditional `Replace` (its own enclosing Conditional's
/// xpath differs from its own) never counts as `B` — no edge, no
/// finding.
#[test]
fn replace_discards_addition_genuinely_conditional_replace_does_not_emit() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        conditional_xpath: Some(r#"Defs/ThingDef[defName="Wall"]"#.to_string()),
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// An adder op sitting under a default-off mod-setting toggle never
/// runs, so it must never be counted as a candidate `A`.
#[test]
fn replace_discards_addition_toggle_inactive_adder_does_not_emit() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        toggle_active: false,
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// A mod's own addition next to its own replace is never a candidate
/// pair — same-mod pairs are excluded outright.
#[test]
fn replace_discards_addition_same_mod_pair_does_not_emit() {
    let mut one = scanned(bare_mod("one", "One"), vec![]);
    one.patch_ops = vec![
        PatchOp {
            value_root_names: vec!["comps".to_string()],
            value_digest: Some(ValueDigest::default()),
            ..touching_op(
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/comps"#,
            )
        },
        PatchOp {
            value_digest: Some(digest(&[("li", 1)])),
            ..touching_op(
                "PatchOperationAdd",
                r#"Defs/ThingDef[defName="Wall"]/comps"#,
            )
        },
    ];
    let scanned_mods = vec![one];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}

/// An `AttributeAdd`'s own `<value>` is a scalar (no element digest at
/// all), which can never be a "duplicate" of anything — the edge is
/// still emitted.
#[test]
fn replace_discards_addition_attribute_add_with_no_element_digest_still_emits() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationAttributeAdd",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, _conflicts) = replace_discards_addition_of(&scanned_mods);

    assert_eq!(edges.len(), 1);
}

/// `PatchOperationInsert` is deliberately not one of the discarded-addition pass's additive
/// classes any more — its own required anchor makes it
/// `patch_removed_node_edges`'s Replace-remover pass's own domain instead
/// (see `READING_CLASS_SUFFIXES`'s own doc comment), so it must never also
/// source a discarded-addition edge in the opposite direction for the same pair.
#[test]
fn replace_discards_addition_insert_class_does_not_emit_its_own_reading_pass_handles_it() {
    let mut replacer = scanned(bare_mod("replacer", "Replacer"), vec![]);
    replacer.patch_ops = vec![PatchOp {
        value_root_names: vec!["comps".to_string()],
        value_digest: Some(ValueDigest::default()),
        ..touching_op(
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let mut adder = scanned(bare_mod("adder", "Adder"), vec![]);
    adder.patch_ops = vec![PatchOp {
        value_digest: Some(digest(&[("li", 1)])),
        ..touching_op(
            "PatchOperationInsert",
            r#"Defs/ThingDef[defName="Wall"]/comps"#,
        )
    }];
    let scanned_mods = vec![replacer, adder];

    let (edges, conflicts) = replace_discards_addition_of(&scanned_mods);

    assert!(edges.is_empty(), "edges: {edges:?}");
    assert!(conflicts.is_empty(), "conflicts: {conflicts:?}");
}
