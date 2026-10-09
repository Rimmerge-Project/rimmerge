//! Assembles the full [`Report`] from a completed scan.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::{
    Conflict, Constraint, Edge, EdgeReport, GameVersion, ModId, Report, ReportMetadata, ScanOutput,
    ScannedMod, UnresolvedFindMod, Warning,
};

use super::{
    checks, conflicts, edges, framework_score,
    indices::{ActiveMods, DefKey, DisplayNameIndex, Indices},
    inheritance::{TemplateRegistrations, broken_inheritance},
    mod_cost, order_check, references,
    source_index::{ChildrenIndex, build_children_index},
};

/// The paths and version used for a run, echoed into
/// [`ReportMetadata`](crate::domain::ReportMetadata).
pub struct RunContext {
    pub game_dir: PathBuf,
    pub workshop_dir: PathBuf,
    pub mods_config_path: PathBuf,
    pub game_version: GameVersion,
}

/// Runs every edge, conflict, and sanity check over a completed scan and
/// assembles the result into one [`Report`] — taking `scan` by value for
/// callers who no longer need it afterward. Delegates to [`build_ref`],
/// which is worth calling directly instead when a caller *does* still need
/// `scan` (e.g. its `ScannedMod`s, for downstream tag-evidence
/// collection): building from a borrow means the caller never has to
/// clone the whole scan (potentially a thousand mods' worth of defs,
/// patches, and assemblies) just to keep a copy around while also handing
/// one to `build`.
#[must_use]
pub fn build(scan: ScanOutput, context: &RunContext) -> Report {
    build_ref(&scan, context)
}

/// [`build`]'s own logic, over a borrowed [`ScanOutput`]. Only
/// `scan.warnings`/`scan.missing_mods` — small, metadata-sized vectors —
/// are cloned to become the owned [`Report`] fields they populate;
/// `scan.scanned_mods` (the expensive part) is never copied, only
/// borrowed, exactly as [`build`] itself already did internally.
#[must_use]
pub fn build_ref(scan: &ScanOutput, context: &RunContext) -> Report {
    let scanned = &scan.scanned_mods;
    // The single definition of "active" for this run: the mods actually
    // found and scanned, never the raw `ModsConfig.xml`/`LoadOrder` list —
    // that list can include entries with no directory on disk, which must
    // never count as active for gating.
    let active = ActiveMods::build(scanned);
    let indices = Indices::build(
        scanned,
        &scan.vanilla_assembly_names,
        &active,
        &scan.core_resource_textures,
    );
    let (name_map, mut warnings) = edges::build_name_map(scanned);
    warnings.extend(scan.warnings.iter().cloned());
    warnings.extend(checks::core_resource_index_warning(
        indices.core_resource_texture_count,
    ));
    // Every mod that patch-injects a whole def, correctly gated by the real
    // `patch_op_active` (needs `name_map`, hence built here rather than as an
    // `Indices` field — see the function's own doc comment). Computed once,
    // shared by `patch_target_edges` and `conflicts::def_overrides` below.
    let injected_owners = edges::injected_def_owners(scanned, &active, &name_map);
    // Who registers which `ParentName` target, by `Verse.XmlInheritance`'s
    // own rules. Shared by the edge producer and `broken_inheritance` below
    // so the two can never disagree about what "registered" means.
    let registrations = TemplateRegistrations::build(scanned, &active, &name_map);

    // The edge side and the conflict side read the same shared inputs and
    // never each other's results, so they run on the rayon pool side by
    // side; each side assembles its own results in a fixed order, so the
    // report never depends on which finishes first.
    let inputs = SharedInputs {
        scan,
        indices: &indices,
        active: &active,
        name_map: &name_map,
        injected_owners: &injected_owners,
        registrations: &registrations,
    };
    let (edge_side, (mut all_conflicts, conflict_warnings)) =
        rayon::join(|| build_edge_side(&inputs), || collect_conflicts(&inputs));
    let EdgeSide {
        edge_reports,
        unresolved_find_mod_names,
        find_mod_names_using_package_id,
        manifest_warnings,
        replace_discards_conflicts,
        constraints,
    } = edge_side;
    warnings.extend(manifest_warnings);
    let undeclared_hard_dependencies = order_check::undeclared_hard_dependencies(&edge_reports);

    let mut mods: Vec<_> = scanned.iter().map(|sm| sm.info.clone()).collect();
    framework_score::apply(&mut mods, &edge_reports);

    warnings.extend(conflict_warnings);
    all_conflicts.extend(replace_discards_conflicts);

    let active_mod_count = scan.load_order.as_slice().len();
    let mod_costs = mod_cost::compute(scanned, &indices, &scan.load_order);

    Report {
        metadata: build_metadata(
            context,
            active_mod_count,
            scan.discovered_mod_count,
            scanned,
            &indices,
        ),
        mods,
        edges: edge_reports,
        conflicts: all_conflicts,
        constraints,
        undeclared_hard_dependencies,
        missing_mods: scan.missing_mods.clone(),
        missing_dependencies: checks::missing_dependencies(scanned, &active),
        incompatible_active_pairs: checks::incompatible_active_pairs(scanned, &active),
        unsupported_version_mods: checks::unsupported_version_mods(scanned, context.game_version),
        unresolved_find_mod_names,
        find_mod_names_using_package_id,
        warnings,
        mod_costs,
        inactive_mods: scan.inactive_mods.clone(),
    }
}

/// What [`build_ref`]'s edge side produces: every evaluated edge, the
/// `PatchOperationFindMod` name-resolution byproducts, the manifest's own
/// warnings, the discarded-addition conflicts (one pass yields both its
/// edges and those), and the assembly-reference constraints.
struct EdgeSide {
    edge_reports: Vec<EdgeReport>,
    unresolved_find_mod_names: Vec<UnresolvedFindMod>,
    find_mod_names_using_package_id: Vec<UnresolvedFindMod>,
    manifest_warnings: Vec<Warning>,
    replace_discards_conflicts: Vec<Conflict>,
    constraints: Vec<Constraint>,
}

/// Everything [`build_ref`]'s edge and conflict sides both read, built
/// once before either starts.
struct SharedInputs<'a> {
    scan: &'a ScanOutput,
    indices: &'a Indices,
    active: &'a ActiveMods,
    name_map: &'a DisplayNameIndex,
    injected_owners: &'a BTreeMap<DefKey, Vec<ModId>>,
    registrations: &'a TemplateRegistrations,
}

/// The edge half of [`build_ref`], sequential within itself: the manifest
/// and def-override producers read the edges built before them.
fn build_edge_side(inputs: &SharedInputs<'_>) -> EdgeSide {
    let SharedInputs {
        scan,
        indices,
        active,
        name_map,
        injected_owners,
        registrations,
    } = *inputs;
    let scanned = &scan.scanned_mods;
    let (
        mut all_edges,
        unresolved_find_mod_names,
        find_mod_names_using_package_id,
        manifest_warnings,
    ) = collect_edges(
        scanned,
        indices,
        active,
        name_map,
        injected_owners,
        registrations,
    );
    // Discarded additions: a later `Replace` discarding an earlier active mod's own
    // addition. Shares its own edges/conflicts pair from one pass, since
    // the owner-declared-override rule (rule 3) decides per candidate
    // pair whether the fact surfaces as an edge or only as a finding —
    // computed here, not inside `collect_edges`/`collect_conflicts`,
    // specifically so both halves come from that single pass.
    let (replace_discards_edges, replace_discards_conflicts) =
        edges::replace_discards_addition(scanned, indices, active, name_map);
    all_edges.extend(replace_discards_edges);
    EdgeSide {
        edge_reports: order_check::evaluate(all_edges, &scan.load_order),
        unresolved_find_mod_names,
        find_mod_names_using_package_id,
        manifest_warnings,
        replace_discards_conflicts,
        constraints: edges::assembly_ref_constraints(scanned, indices, &scan.load_order),
    }
}

/// Every [`Edge`] between active mods, plus the `PatchOperationFindMod`
/// name-resolution byproducts that ride along with [`edges::find_mod_edges`].
fn collect_edges(
    scanned: &[ScannedMod],
    indices: &Indices,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
    registrations: &TemplateRegistrations,
) -> (
    Vec<Edge>,
    Vec<UnresolvedFindMod>,
    Vec<UnresolvedFindMod>,
    Vec<Warning>,
) {
    let mut all_edges = Vec::new();
    all_edges.extend(edges::assembly_ref_edges(scanned, indices));
    all_edges.extend(edges::declared_edges(scanned, active));
    let find_mod_resolution = edges::find_mod_edges(scanned, name_map, active);
    all_edges.extend(find_mod_resolution.edges);
    all_edges.extend(edges::if_mod_active_edges(scanned, active));
    all_edges.extend(edges::patch_target_edges(
        scanned,
        indices,
        active,
        name_map,
        injected_owners,
    ));
    all_edges.extend(edges::may_require_edges(scanned, active));
    all_edges.extend(edges::patch_injected_node_edges(
        scanned, indices, active, name_map,
    ));
    all_edges.extend(edges::uses_type_edges(scanned, indices));
    all_edges.extend(edges::parent_template_edges(scanned, active, registrations));
    all_edges.extend(edges::assembly_version_precedence_edges(indices));
    // "Remover loads last" — see that function's own doc comment for the full
    // rule and its two disclosed known gaps.
    all_edges.extend(edges::patch_removed_node_edges(scanned, active, name_map));
    // A Replace/Remove that invalidates the predicate another active mutating
    // op depends on. Reads the same per-op data as
    // `patch_removed_node_edges`, so it sits directly beside it.
    all_edges.extend(edges::patch_invalidates_predicate_edges(
        scanned, active, name_map,
    ));
    // Texture-only mods load after the content they retexture — no dependency
    // on any edge built above.
    all_edges.extend(edges::retexture_after_owner_edges(scanned, indices));
    // `About/Manifest.xml` load-order hints — seeded with `all_edges` so far
    // specifically to dedupe against `declared_edges`'s own
    // `About.xml`-sourced `LoadAfter`/`LoadBefore`/`ModDependency` edges (see
    // the function's own doc comment); its own unresolved-name warnings ride
    // along in the returned tuple's second element, appended by the caller.
    let (manifest_edges, manifest_warnings) =
        edges::manifest_order_edges(scanned, active, name_map, &all_edges);
    all_edges.extend(manifest_edges);
    // Infer a def-override origin — runs last on purpose: its own "no
    // Declared/Hard edge between the owners" gate needs every other
    // producer's edges (including the manifest ones, just above) already in
    // `all_edges` to check against.
    all_edges.extend(edges::def_override_after_origin_edges(
        scanned, indices, &all_edges,
    ));
    (
        all_edges,
        find_mod_resolution.unresolved,
        find_mod_resolution.using_package_id,
        manifest_warnings,
    )
}

/// Every [`Conflict`] between active mods. `registrations` is the same
/// [`TemplateRegistrations`] the edge side already uses, shared here
/// (never rebuilt) so `broken_inheritance` and `parent_template_edges`
/// can never disagree about what "registered" means.
///
/// The two checks that need the children index (broken inheritance and
/// the dangling-reference vote, the slowest producer) run beside every
/// other producer; the results are appended in one fixed order.
fn collect_conflicts(inputs: &SharedInputs<'_>) -> (Vec<Conflict>, Vec<Warning>) {
    let SharedInputs {
        scan,
        active,
        registrations,
        ..
    } = *inputs;
    let scanned = &scan.scanned_mods;
    let ((mut all_conflicts, near_miss), (broken_inheritance_side, dangling)) = rayon::join(
        || {
            (
                independent_conflicts(inputs),
                checks::near_miss_mod_references(scanned, active, &scan.inactive_mods),
            )
        },
        || {
            let children_index = build_children_index(scan);
            rayon::join(
                || {
                    broken_inheritance(
                        scanned,
                        active,
                        registrations,
                        &children_index,
                        &scan.vanilla_type_hierarchy,
                    )
                },
                || dangling_def_references(inputs, &children_index),
            )
        },
    );
    let (broken_inheritance_conflicts, broken_inheritance_warnings) = broken_inheritance_side;
    all_conflicts.extend(broken_inheritance_conflicts);
    all_conflicts.extend(near_miss);
    all_conflicts.extend(dangling);
    (all_conflicts, broken_inheritance_warnings)
}

/// Every conflict producer that needs nothing but the shared inputs, in
/// report order.
fn independent_conflicts(inputs: &SharedInputs<'_>) -> Vec<Conflict> {
    let SharedInputs {
        scan,
        indices,
        active,
        name_map,
        injected_owners,
        ..
    } = *inputs;
    let scanned = &scan.scanned_mods;
    let load_order = &scan.load_order;
    let mut all_conflicts = Vec::new();
    all_conflicts.extend(conflicts::def_overrides(
        indices,
        load_order,
        injected_owners,
    ));
    all_conflicts.extend(conflicts::patch_collisions(
        scanned, load_order, active, name_map,
    ));
    all_conflicts.extend(conflicts::texture_overrides(indices, load_order));
    all_conflicts.extend(conflicts::duplicate_assemblies(indices, load_order));
    all_conflicts.extend(conflicts::likely_duplicate_mods(scanned, indices));
    all_conflicts.extend(conflicts::duplicate_template_names(indices, load_order));
    all_conflicts.extend(conflicts::keyed_translation_collisions(indices, load_order));
    all_conflicts.extend(conflicts::sound_overrides(indices, load_order));
    all_conflicts.extend(conflicts::runtime_patch_collisions(indices, load_order));
    all_conflicts.extend(conflicts::transpiler_collisions(indices, load_order));
    all_conflicts.extend(conflicts::missing_texture_paths(scanned, indices, active));
    all_conflicts.extend(conflicts::undecodable_textures(scanned));
    all_conflicts
}

/// The dangling-def-reference vote. The `RemovedBy` cause is decided
/// here, from data this pure pass already has; every other dangling name
/// comes out `DanglingCause::Unexplained` —
/// `infra::explain_dangling_references` is the lazy IO pass a composition
/// root runs afterward to fill in the rest (see `analysis::references`'s
/// own doc comment for why the split exists).
fn dangling_def_references(
    inputs: &SharedInputs<'_>,
    children_index: &ChildrenIndex,
) -> Vec<Conflict> {
    references::dangling_def_references(
        &inputs.scan.scanned_mods,
        inputs.active,
        inputs.name_map,
        inputs.indices,
        inputs.injected_owners,
        children_index,
        &inputs.scan.ref_sites_by_mod,
    )
}

fn build_metadata(
    context: &RunContext,
    active_mod_count: usize,
    discovered_mod_count: usize,
    scanned: &[ScannedMod],
    indices: &Indices,
) -> ReportMetadata {
    ReportMetadata {
        schema_version: crate::domain::REPORT_SCHEMA_VERSION,
        game_dir: context.game_dir.clone(),
        workshop_dir: context.workshop_dir.clone(),
        mods_config: context.mods_config_path.clone(),
        game_version: context.game_version.to_string(),
        generated_at: now_rfc3339(),
        active_mod_count,
        scanned_mod_count: scanned.len(),
        mods_with_assemblies: scanned
            .iter()
            .filter(|sm| !sm.assemblies.is_empty())
            .count(),
        mods_with_patches: scanned.iter().filter(|sm| !sm.patch_ops.is_empty()).count(),
        mods_with_defs: scanned.iter().filter(|sm| !sm.defs.is_empty()).count(),
        total_defs_indexed: scanned.iter().map(|sm| sm.defs.len()).sum(),
        distinct_texture_paths: indices.texture_owners.len(),
        discovered_mod_count,
        core_resource_texture_count: indices.core_resource_texture_count,
    }
}

fn now_rfc3339() -> String {
    jiff::Timestamp::now().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AssemblyInfo, DeclaredOrder, DefEntry, DefTarget, InactiveMod, LoadOrder, Mod, ModId,
        PatchOp, RefSite, RefSiteOwner, RefSiteShape, ScannedMod, Selector, Source, TemplateEntry,
        XmlLocator,
    };
    use std::collections::{BTreeSet, HashSet};
    use std::path::PathBuf;

    fn mod_info(id: &str, author: &str) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: vec![author.to_string()],
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

    fn patch_op(def_type: &str, def_name: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: None,
            target: Some(DefTarget {
                def_type: def_type.to_string(),
                def_name: def_name.to_string(),
                selector: Selector::DefName,
                sub_path: None,
            }),
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

    fn assembly(name: &str, references: Vec<&str>) -> AssemblyInfo {
        AssemblyInfo {
            file_name: name.to_string(),
            name: name.to_string(),
            references: references
                .into_iter()
                .map(|name| crate::domain::AssemblyReference {
                    name: name.to_string(),
                    load_time: true,
                })
                .collect(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }
    }

    /// Like [`def_entry`], but with a `ParentName` — for exercising
    /// [`crate::analysis::edges::parent_template_edges`].
    fn def_entry_with_parent(def_type: &str, def_name: &str, parent_name: &str) -> DefEntry {
        DefEntry {
            parent_name: Some(parent_name.to_string()),
            ..def_entry(def_type, def_name)
        }
    }

    fn template_entry(name: &str, def_type: &str) -> TemplateEntry {
        TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            def_type: def_type.to_string(),
            name: name.to_string(),
            parent_name: None,
            is_abstract: true,
            locator: XmlLocator::for_test(),
        }
    }

    /// A mutating patch op injecting one class via `<value>` — for exercising
    /// [`crate::analysis::edges::patch_injected_node_edges`].
    fn injecting_patch_op(def_type: &str, def_name: &str, injected: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            injected_types: BTreeSet::from([injected.to_string()]),
            ..patch_op(def_type, def_name)
        }
    }

    /// A mutating patch op whose xpath selects one namespace-qualified class
    /// — for exercising
    /// [`crate::analysis::edges::patch_injected_node_edges`].
    fn selecting_patch_op(xpath: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            xpath: Some(xpath.to_string()),
            target: None,
            ..patch_op("ThingDef", "Unused")
        }
    }

    /// A scan with enough distinct owner keys (defs, patch targets, textures,
    /// duplicate assemblies) and enough distinct assembly references from one
    /// mod that a `HashMap`/`HashSet`-backed index would very likely disagree
    /// with itself between two runs on their relative order — the regression
    /// this test guards against. Also carries a mod with five `ParentName`s
    /// onto the same owner (`mod.epsilon` -> `mod.zeta`) and a mod selecting
    /// five classes injected by the same injector (`mod.theta` -> `mod.eta`):
    /// both collapse onto a single edge whose detail text names whichever one
    /// a `HashSet`-backed iteration visited first, so this scan also guards
    /// against that specific class of nondeterminism.
    fn hand_made_scan() -> ScanOutput {
        let mut a = ScannedMod {
            info: mod_info("mod.a", "Alice"),
            defs: vec![def_entry("ThingDef", "Wall"), def_entry("ThingDef", "Door")],
            templates: Vec::new(),
            patch_ops: vec![patch_op("ThingDef", "Wall"), patch_op("ThingDef", "Door")],
            textures: BTreeMap::from([
                ("things/wall".to_string(), 0),
                ("things/door".to_string(), 0),
            ]),
            assemblies: vec![
                assembly("sharedlib1", vec![]),
                assembly("sharedlib2", vec![]),
            ],
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
        };
        a.info.declared.load_after = vec![ModId::new("mod.b")];

        let b = ScannedMod {
            info: mod_info("mod.b", "Bob"),
            defs: vec![def_entry("ThingDef", "Wall"), def_entry("ThingDef", "Door")],
            templates: Vec::new(),
            patch_ops: vec![patch_op("ThingDef", "Wall"), patch_op("ThingDef", "Door")],
            textures: BTreeMap::from([
                ("things/wall".to_string(), 0),
                ("things/door".to_string(), 0),
            ]),
            assemblies: vec![
                assembly("sharedlib1", vec![]),
                assembly("sharedlib2", vec![]),
            ],
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
        };

        let c = ScannedMod {
            info: mod_info("mod.c", "Carol"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: vec![assembly(
                "referencer",
                vec![
                    "sharedlib1",
                    "sharedlib2",
                    "libalpha",
                    "libbeta",
                    "libgamma",
                    "libdelta",
                ],
            )],
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
        };

        let alpha = ScannedMod {
            info: mod_info("mod.alpha", "Dave"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: vec![assembly("libalpha", vec![])],
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
        };
        // `beta`/`gamma` also each ship an `XAM_`-prefixed def, no
        // `Declared`/`Hard` edge between them (their only edges are each
        // owning one library `mod.c`'s own assembly references, never each
        // other): exercises `def_override_after_origin_edges` inside the
        // determinism fixture.
        let beta = ScannedMod {
            info: mod_info("mod.beta", "Eve"),
            defs: vec![
                def_entry("ThingDef", "XAM_Wall"),
                def_entry("ThingDef", "XAM_Table"),
            ],
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: vec![assembly("libbeta", vec![])],
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
        };
        let gamma = ScannedMod {
            info: mod_info("mod.gamma", "Frank"),
            defs: vec![def_entry("ThingDef", "XAM_Wall")],
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: vec![assembly("libgamma", vec![])],
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
        };
        // Also carries a Manifest.xml entry naming `mod.a` by its display
        // name ("Alice"): exercises `manifest_order_edges` inside the
        // determinism fixture.
        let delta = ScannedMod {
            info: mod_info("mod.delta", "Grace"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: vec![assembly("libdelta", vec![])],
            sounds: std::collections::BTreeSet::new(),
            translation_keys: std::collections::BTreeSet::new(),
            inline_types: std::collections::BTreeSet::new(),
            manifest_order: crate::domain::ManifestOrder {
                load_after: vec!["Alice".to_string()],
                load_before: Vec::new(),
                dependencies: Vec::new(),
            },
            texture_path_candidates: Vec::new(),
            inline_node_path_hashes: std::collections::HashSet::new(),
            if_mod_active_targets: Vec::new(),
            scan_cost: crate::domain::ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        };
        // Texture-only (no defs/templates/patch_ops/assemblies), sharing
        // `mod.a`/`mod.b`'s own "things/wall" texture: exercises
        // `retexture_after_owner_edges` inside the determinism fixture.
        let iota = ScannedMod {
            info: mod_info("mod.iota", "Iota"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::from([("things/wall".to_string(), 0)]),
            assemblies: Vec::new(),
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
        };

        // Registers five `Name`-attributed templates, all owned by this
        // single mod — `mod.epsilon` below inherits from all five, so
        // `parent_template_edges` collapses them onto one edge and the
        // detail text depends on which `ParentName` its (formerly)
        // `HashSet`-backed iteration visited first. Five, not two: with
        // only two candidates a `HashSet`'s per-process-random order
        // still has a coin-flip chance of matching between the test's two
        // `build()` calls, which would let the bug slip past this
        // regression guard undetected on an unlucky run.
        let zeta = ScannedMod {
            info: mod_info("mod.zeta", "Heidi"),
            defs: Vec::new(),
            templates: vec![
                template_entry("TemplateOne", "ThingDef"),
                template_entry("TemplateTwo", "ThingDef"),
                template_entry("TemplateThree", "ThingDef"),
                template_entry("TemplateFour", "ThingDef"),
                template_entry("TemplateFive", "ThingDef"),
            ],
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
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
        };
        let epsilon = ScannedMod {
            info: mod_info("mod.epsilon", "Ivan"),
            defs: vec![
                def_entry_with_parent("ThingDef", "EpsilonThingOne", "TemplateOne"),
                def_entry_with_parent("ThingDef", "EpsilonThingTwo", "TemplateTwo"),
                def_entry_with_parent("ThingDef", "EpsilonThingThree", "TemplateThree"),
                def_entry_with_parent("ThingDef", "EpsilonThingFour", "TemplateFour"),
                def_entry_with_parent("ThingDef", "EpsilonThingFive", "TemplateFive"),
            ],
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
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
        };

        // Injects five distinct classes, all selected by `mod.theta`
        // below — `patch_injected_node_edges` collapses them onto one
        // `PatchInjectedNode` edge (same injector), and the detail text
        // depends on which selected class its (formerly) `HashSet`-backed
        // iteration visited first. Five, not two, for the same
        // coin-flip-avoidance reason as `mod.zeta`/`mod.epsilon` above.
        let eta = ScannedMod {
            info: mod_info("mod.eta", "Judy"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: vec![
                injecting_patch_op("ThingDef", "EtaTargetA", "Injected.ClassAlpha"),
                injecting_patch_op("ThingDef", "EtaTargetB", "Injected.ClassBeta"),
                injecting_patch_op("ThingDef", "EtaTargetC", "Injected.ClassGamma"),
                injecting_patch_op("ThingDef", "EtaTargetD", "Injected.ClassDelta"),
                injecting_patch_op("ThingDef", "EtaTargetE", "Injected.ClassEpsilon"),
            ],
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
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
        };
        let theta = ScannedMod {
            info: mod_info("mod.theta", "Kevin"),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: vec![
                selecting_patch_op(
                    r#"Defs/ThingDef[defName="ThetaX"]/modExtensions/li[@Class="Injected.ClassAlpha"]"#,
                ),
                selecting_patch_op(
                    r#"Defs/ThingDef[defName="ThetaY"]/modExtensions/li[@Class="Injected.ClassBeta"]"#,
                ),
                selecting_patch_op(
                    r#"Defs/ThingDef[defName="ThetaZ"]/modExtensions/li[@Class="Injected.ClassGamma"]"#,
                ),
                selecting_patch_op(
                    r#"Defs/ThingDef[defName="ThetaW"]/modExtensions/li[@Class="Injected.ClassDelta"]"#,
                ),
                selecting_patch_op(
                    r#"Defs/ThingDef[defName="ThetaV"]/modExtensions/li[@Class="Injected.ClassEpsilon"]"#,
                ),
            ],
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
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
        };

        let order = vec![
            ModId::new("mod.a"),
            ModId::new("mod.b"),
            ModId::new("mod.c"),
            ModId::new("mod.alpha"),
            ModId::new("mod.beta"),
            ModId::new("mod.gamma"),
            ModId::new("mod.delta"),
            ModId::new("mod.zeta"),
            ModId::new("mod.epsilon"),
            ModId::new("mod.eta"),
            ModId::new("mod.theta"),
            ModId::new("mod.iota"),
        ];

        let scanned_mods = vec![
            a, b, c, alpha, beta, gamma, delta, zeta, epsilon, eta, theta, iota,
        ];
        // Two entries, given in reverse-of-sorted insertion order —
        // `ScanOutput.inactive_mods` is trusted pre-sorted (see that field's
        // own doc comment), never re-sorted by `build_ref`, so this is what
        // actually exercises
        // `building_the_same_scan_twice_produces_byte_identical_json`
        // covering the field at all; an empty `Vec` would make that test's
        // own claim to cover this field vacuous.
        let inactive_mods = vec![
            inactive_mod("mod.zzz.inactive"),
            inactive_mod("mod.aaa.inactive"),
        ];
        let discovered_mod_count = scanned_mods.len() + inactive_mods.len();
        ScanOutput {
            scanned_mods,
            load_order: LoadOrder::new(order),
            missing_mods: Vec::new(),
            vanilla_assembly_names: HashSet::new(),
            vanilla_type_hierarchy: Vec::new(),
            warnings: Vec::new(),
            child_value_hashes_by_mod: BTreeMap::new(),
            inactive_mods,
            discovered_mod_count,
            // Large enough to clear `MIN_CORE_RESOURCE_TEXTURES`, so this
            // fixture exercises `missing_texture_path`'s ordinary path
            // rather than its self-disabling gate (and so its own
            // `core_resource_index_warning` never shows up as spurious
            // extra warnings in this test's JSON comparison).
            core_resource_textures: (0..crate::analysis::indices::MIN_CORE_RESOURCE_TEXTURES)
                .map(|i| format!("synthetic/{i}"))
                .collect(),
            ref_sites_by_mod: BTreeMap::new(),
        }
    }

    fn inactive_mod(id: &str) -> InactiveMod {
        InactiveMod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            path: PathBuf::from(id),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        }
    }

    fn context() -> RunContext {
        RunContext {
            game_dir: PathBuf::from("game"),
            workshop_dir: PathBuf::from("workshop"),
            mods_config_path: PathBuf::from("ModsConfig.xml"),
            game_version: GameVersion::new(1, 6),
        }
    }

    /// Normalizes away the one field that's expected to differ between
    /// two runs (`generated_at`) so the rest of the report can be
    /// compared for exact, order-sensitive equality.
    fn normalized_json(report: &Report) -> serde_json::Value {
        let mut value = serde_json::to_value(report).expect("Report must serialize");
        value["metadata"]["generated_at"] = serde_json::Value::Null;
        value
    }

    #[test]
    fn building_the_same_scan_twice_produces_byte_identical_json() {
        let context = context();

        let report_one = build(hand_made_scan(), &context);
        let report_two = build(hand_made_scan(), &context);

        assert_eq!(normalized_json(&report_one), normalized_json(&report_two));
    }

    /// `build_ref` must produce exactly the same `Report` as `build` (which
    /// now delegates to it) — the only difference is that `build_ref`
    /// borrows `scan` instead of consuming it, so the caller keeps its own
    /// copy (e.g. for downstream tag-evidence collection over
    /// `ScannedMod`s) without cloning the whole scan first.
    #[test]
    fn build_ref_matches_build_and_leaves_the_scan_usable_afterward() {
        let context = context();
        let scan = hand_made_scan();

        let via_ref = build_ref(&scan, &context);

        // `scan` is still owned and usable here — `build_ref` only
        // borrowed it.
        assert_eq!(scan.scanned_mods.len(), 12);

        let via_value = build(scan, &context);

        assert_eq!(normalized_json(&via_ref), normalized_json(&via_value));
    }

    /// `collect_conflicts` runs its producers on two sides in parallel and
    /// then appends their results; `Report.conflicts` is never re-sorted,
    /// so that append order *is* the report's order. This pins it: every
    /// sequential producer's conflicts first, then broken inheritance, then
    /// the near-miss mod references, then the dangling def references.
    #[test]
    fn conflicts_keep_their_producer_order_across_the_parallel_sides() {
        let mut scan = hand_made_scan();
        let first = &mut scan.scanned_mods[0];
        first
            .defs
            .push(def_entry_with_parent("ThingDef", "Orphan", "NoSuchBase"));
        first.defs.push(DefEntry {
            may_require: vec!["mod.epsilno".to_string()],
            ..def_entry("ThingDef", "Gated")
        });
        // A hyperlink site needs no vote, so one site is enough for a
        // dangling reference from an active mod's own def.
        scan.ref_sites_by_mod.insert(
            ModId::new("mod.a"),
            vec![RefSite {
                def_type: std::sync::Arc::from("ThingDef"),
                field_path: std::sync::Arc::from("descriptionHyperlinks"),
                shape: RefSiteShape::Hyperlink,
                value: "GhostThing".to_string(),
                owner: std::sync::Arc::new(RefSiteOwner::Def {
                    def_name: "Wall".to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                }),
                may_require: Box::default(),
                may_require_any_of: Box::default(),
            }],
        );

        let report = build(scan, &context());

        let kinds: Vec<String> = report
            .conflicts
            .iter()
            .map(|conflict| {
                serde_json::to_value(conflict).expect("a conflict serializes")["kind"]
                    .as_str()
                    .expect("every conflict is tagged with its kind")
                    .to_string()
            })
            .collect();
        let rank = |kind: &str| match kind {
            "broken_inheritance" => 1,
            "near_miss_mod_reference" => 2,
            "dangling_def_reference" => 3,
            _ => 0,
        };
        let ranks: Vec<u8> = kinds.iter().map(|kind| rank(kind)).collect();
        assert!(
            [1, 2, 3].iter().all(|wanted| ranks.contains(wanted)),
            "{kinds:?}"
        );
        assert!(ranks.is_sorted(), "out of producer order: {kinds:?}");
    }

    /// `Report` (and every type reachable from it) now derives
    /// `Deserialize` alongside its existing `Serialize`, so a cached
    /// `report.json` can be loaded back without re-scanning. This must
    /// round-trip losslessly: serializing, parsing back, and
    /// re-serializing must produce the same JSON.
    #[test]
    fn report_survives_a_serde_json_roundtrip() {
        let report = build(hand_made_scan(), &context());

        let json = serde_json::to_string(&report).expect("Report must serialize");
        let reloaded: Report = serde_json::from_str(&json).expect("Report must deserialize");

        assert_eq!(normalized_json(&report), normalized_json(&reloaded));
    }
}
