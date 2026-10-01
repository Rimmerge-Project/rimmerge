//! Computes one [`ModCost`] per scanned (active) mod from data the scan
//! already gathered — no new IO, no timings (see [`ModCost`]'s own doc
//! comment for why).

use crate::domain::{LoadOrder, ModCost, ScannedMod};
use crate::extract::xpath_expr;

use super::conflicts::sorted_by_load_order;
use super::indices::Indices;

/// One [`ModCost`] per mod in `scanned`, in the same order (scan order —
/// matching every other per-mod [`crate::domain::Report`] vector, not
/// sorted by [`crate::domain::ModId`]).
#[must_use]
pub fn compute(scanned: &[ScannedMod], indices: &Indices, load_order: &LoadOrder) -> Vec<ModCost> {
    scanned
        .iter()
        .map(|scanned_mod| mod_cost(scanned_mod, indices, load_order))
        .collect()
}

fn mod_cost(scanned_mod: &ScannedMod, indices: &Indices, load_order: &LoadOrder) -> ModCost {
    let patch_ops = scanned_mod
        .patch_ops
        .iter()
        .filter(|op| op.is_mutating)
        .count();
    // Spans *every* patch op, mutating or not — unlike `patch_ops` above. A
    // `PatchOperationConditional` node is itself emitted as a non-mutating
    // `PatchOp` carrying its own gating `<xpath>` (`class::PatchOp`'s own doc
    // comment lists it among the control-flow-only classes), and RimWorld
    // evaluates that gate regardless of whether the node itself ever mutates
    // anything — exactly the slow shape this column exists to surface. A
    // nested mutating leaf under a slow Conditional is never double-counted:
    // each `PatchOp` (the Conditional's own node and the leaf beneath it
    // alike) contributes only its *own* `xpath` field, never
    // `conditional_xpath` (the enclosing gate's xpath, duplicated onto every
    // descendant for other purposes) — so one Conditional gating five
    // mutating ops still counts once, and each of those five leaves is
    // independently judged on its own, different target xpath.
    let slow_xpath_ops = scanned_mod
        .patch_ops
        .iter()
        .filter(|op| op.xpath.as_deref().is_some_and(xpath_expr::is_slow_shape))
        .count();

    ModCost {
        mod_id: scanned_mod.info.id.clone(),
        patch_ops,
        slow_xpath_ops,
        texture_files: scanned_mod.scan_cost.texture_files,
        texture_bytes: scanned_mod.scan_cost.texture_bytes,
        dds_files: scanned_mod.scan_cost.dds_files,
        assembly_count: scanned_mod.assemblies.len(),
        assembly_bytes: scanned_mod.scan_cost.assembly_bytes,
        def_count: scanned_mod.defs.len(),
        content_only: patch_ops == 0 && scanned_mod.assemblies.is_empty(),
        overridden_texture_bytes: overridden_texture_bytes(scanned_mod, indices, load_order),
        nameless_def_count: scanned_mod.nameless_def_count,
    }
}

/// Sum of this mod's own texture bytes for every key it shares with at
/// least one other owner where this mod isn't the last one loaded — the
/// game loads this mod's copy, then immediately discards it in favor of
/// whichever owner loads after it.
///
/// **Disclosed double-count**: a mod shipping the *same* normalized key from
/// two of its own loaded folders (e.g. `Wall.png` at both its root and its
/// `1.6/` folder) has `ScannedMod::textures`'s value for that key already
/// summed across both files (`infra::mod_scan::scan_folder` accumulates every
/// loaded folder into one map, by design — see that field's own doc comment),
/// so this function charges *both* copies as overridden even though only one
/// of the two is ever actually the one thrown away (the other was never the
/// winner in the first place, and both are the same mod's own content — not a
/// real "duplicate override" the way two different mods sharing a key is).
/// Not fixed here: the fix would need per-file rather than per-key texture
/// bytes, a bigger shape change than this diagnostic-only column justifies.
fn overridden_texture_bytes(
    scanned_mod: &ScannedMod,
    indices: &Indices,
    load_order: &LoadOrder,
) -> u64 {
    let this_id = &scanned_mod.info.id;
    scanned_mod
        .textures
        .iter()
        .filter(|(key, _)| {
            let Some(owners) = indices.texture_owners.get(*key) else {
                return false;
            };
            if owners.len() < 2 {
                return false;
            }
            let sorted_owners = sorted_by_load_order(owners, load_order);
            sorted_owners.last() != Some(this_id)
        })
        .map(|(_, bytes)| *bytes)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AssemblyInfo, DeclaredOrder, DefEntry, DefTarget, Mod, ModId, PatchOp, Selector, Source,
        XmlLocator,
    };
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::path::PathBuf;

    fn mod_info(id: &str) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
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

    fn bare_mod(id: &str) -> ScannedMod {
        ScannedMod {
            info: mod_info(id),
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            sounds: BTreeSet::new(),
            translation_keys: BTreeSet::new(),
            inline_types: BTreeSet::new(),
            manifest_order: Default::default(),
            inline_node_path_hashes: HashSet::new(),
            assemblies: Vec::new(),
            if_mod_active_targets: Vec::new(),
            texture_path_candidates: Vec::new(),
            scan_cost: Default::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
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

    fn mutating_op(xpath: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(xpath.to_string()),
            target: Some(DefTarget {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                selector: Selector::DefName,
                sub_path: None,
            }),
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
            locator: XmlLocator::for_test(),
        }
    }

    fn control_flow_op() -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationFindMod".to_string(),
            xpath: None,
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: vec!["Some Mod".to_string()],
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: false,
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
            locator: XmlLocator::for_test(),
        }
    }

    /// A `PatchOperationConditional` node: non-mutating, but — unlike
    /// [`control_flow_op`] — carries its own gating `xpath`, exactly the
    /// shape `slow_xpath_ops` exists to surface.
    fn conditional_op(xpath: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationConditional".to_string(),
            xpath: Some(xpath.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: false,
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
            locator: XmlLocator::for_test(),
        }
    }

    fn build_indices(scanned: &[ScannedMod]) -> Indices {
        use super::super::indices::ActiveMods;
        Indices::build(
            scanned,
            &HashSet::new(),
            &ActiveMods::build(scanned),
            &std::collections::BTreeSet::new(),
        )
    }

    #[test]
    fn patch_ops_counts_mutating_ops_only_slow_xpath_ops_counts_any_op_with_a_slow_xpath() {
        let mut a = bare_mod("a");
        a.patch_ops = vec![
            mutating_op(r#"Defs/ThingDef[defName="Wall"]/comps/li[1]"#),
            mutating_op(r#"Defs//ThingDef[defName="Wall"]"#),
            control_flow_op(),
        ];
        let indices = build_indices(&[a.clone()]);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&[a], &indices, &load_order);

        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0].patch_ops, 2, "control-flow node excluded");
        assert_eq!(costs[0].slow_xpath_ops, 1, "only the `//` xpath is slow");
    }

    /// A `PatchOperationConditional` gate is itself non-mutating (excluded
    /// from `patch_ops`, same as any other control-flow node) but its own
    /// `xpath` is exactly what RimWorld evaluates to decide whether to run
    /// its branch — `slow_xpath_ops` must count it, proving the two columns'
    /// populations genuinely differ rather than merely being documented as
    /// different.
    #[test]
    fn slow_xpath_ops_counts_a_slow_shaped_conditional_gate_even_though_it_is_not_mutating() {
        let mut a = bare_mod("a");
        a.patch_ops = vec![
            conditional_op(r#"Defs//ThingDef[defName="Wall"]"#),
            mutating_op(r#"Defs/ThingDef[defName="Wall"]/comps/li[1]"#),
        ];
        let indices = build_indices(&[a.clone()]);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&[a], &indices, &load_order);

        assert_eq!(
            costs[0].patch_ops, 1,
            "only the mutating leaf counts toward patch_ops"
        );
        assert_eq!(
            costs[0].slow_xpath_ops, 1,
            "the Conditional's own slow gate counts even though it never mutates anything"
        );
    }

    #[test]
    fn content_only_is_true_with_no_mutating_ops_and_no_assemblies() {
        let mut a = bare_mod("a");
        a.patch_ops = vec![control_flow_op()];
        a.defs = vec![def_entry("ThingDef", "Wall")];
        let indices = build_indices(&[a.clone()]);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&[a], &indices, &load_order);

        assert!(costs[0].content_only);
    }

    #[test]
    fn content_only_is_false_when_a_mutating_op_is_present() {
        let mut a = bare_mod("a");
        a.patch_ops = vec![mutating_op(r#"Defs/ThingDef[defName="Wall"]"#)];
        let indices = build_indices(&[a.clone()]);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&[a], &indices, &load_order);

        assert!(!costs[0].content_only);
    }

    #[test]
    fn content_only_is_false_when_an_assembly_is_shipped() {
        let mut a = bare_mod("a");
        a.assemblies = vec![AssemblyInfo {
            file_name: "Mine".to_string(),
            name: "mine".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        }];
        let indices = build_indices(&[a.clone()]);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&[a], &indices, &load_order);

        assert!(!costs[0].content_only);
    }

    /// The earlier-loading owner's texture bytes count as overridden —
    /// the game loads them, then throws them away for the later copy.
    #[test]
    fn overridden_texture_bytes_counts_the_earlier_owners_bytes() {
        let mut a = bare_mod("a");
        a.textures = BTreeMap::from([("things/wall".to_string(), 100)]);
        let mut b = bare_mod("b");
        b.textures = BTreeMap::from([("things/wall".to_string(), 200)]);
        let scanned = vec![a, b];
        let indices = build_indices(&scanned);
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

        let costs = compute(&scanned, &indices, &load_order);

        let a_cost = costs.iter().find(|c| c.mod_id == ModId::new("a")).unwrap();
        assert_eq!(a_cost.overridden_texture_bytes, 100);
    }

    /// The later (winning) owner's own bytes never count as overridden.
    #[test]
    fn overridden_texture_bytes_excludes_the_later_winning_owner() {
        let mut a = bare_mod("a");
        a.textures = BTreeMap::from([("things/wall".to_string(), 100)]);
        let mut b = bare_mod("b");
        b.textures = BTreeMap::from([("things/wall".to_string(), 200)]);
        let scanned = vec![a, b];
        let indices = build_indices(&scanned);
        let load_order = LoadOrder::new(vec![ModId::new("a"), ModId::new("b")]);

        let costs = compute(&scanned, &indices, &load_order);

        let b_cost = costs.iter().find(|c| c.mod_id == ModId::new("b")).unwrap();
        assert_eq!(b_cost.overridden_texture_bytes, 0);
    }

    /// The two tests above both scan `a` before `b` and load `a` before `b`
    /// too — alphabetical order, scan order, and load order all agree, so a
    /// bug that sorted owners by `ModId` instead of consulting `load_order`
    /// at all would still pass them. Flipping the `LoadOrder` alone (same
    /// scan order) puts `b` first — so `b`'s bytes must be the ones charged
    /// as overridden.
    #[test]
    fn overridden_texture_bytes_follows_load_order_not_scan_or_alphabetical_order() {
        let mut a = bare_mod("a");
        a.textures = BTreeMap::from([("things/wall".to_string(), 100)]);
        let mut b = bare_mod("b");
        b.textures = BTreeMap::from([("things/wall".to_string(), 200)]);
        let scanned = vec![a, b];
        let indices = build_indices(&scanned);
        let load_order = LoadOrder::new(vec![ModId::new("b"), ModId::new("a")]);

        let costs = compute(&scanned, &indices, &load_order);

        let a_cost = costs.iter().find(|c| c.mod_id == ModId::new("a")).unwrap();
        let b_cost = costs.iter().find(|c| c.mod_id == ModId::new("b")).unwrap();
        assert_eq!(
            b_cost.overridden_texture_bytes, 200,
            "b loads first under this order, so its own copy is the one discarded"
        );
        assert_eq!(a_cost.overridden_texture_bytes, 0);
    }

    #[test]
    fn overridden_texture_bytes_is_zero_with_a_single_owner() {
        let mut a = bare_mod("a");
        a.textures = BTreeMap::from([("things/wall".to_string(), 100)]);
        let scanned = vec![a];
        let indices = build_indices(&scanned);
        let load_order = LoadOrder::new(vec![ModId::new("a")]);

        let costs = compute(&scanned, &indices, &load_order);

        assert_eq!(costs[0].overridden_texture_bytes, 0);
    }

    #[test]
    fn compute_emits_one_row_per_scanned_mod_in_scan_order() {
        let scanned = vec![bare_mod("z"), bare_mod("a")];
        let indices = build_indices(&scanned);
        let load_order = LoadOrder::new(vec![ModId::new("z"), ModId::new("a")]);

        let costs = compute(&scanned, &indices, &load_order);

        assert_eq!(
            costs.iter().map(|c| c.mod_id.clone()).collect::<Vec<_>>(),
            vec![ModId::new("z"), ModId::new("a")],
            "scan order, not sorted by ModId"
        );
    }
}
