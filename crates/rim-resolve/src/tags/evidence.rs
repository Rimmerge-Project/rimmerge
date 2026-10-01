//! Builds [`TagEvidence`] for every scanned mod: the facts
//! [`super::infer::infer_tags`] matches [`TagSignal`](crate::domain::TagSignal)s
//! against, captured once at scan time so re-running inference after a
//! rule change touches no filesystem.

use std::collections::BTreeSet;

use rim_analyzer::domain::{EdgeKind, Report, ScannedMod};

use crate::domain::{GeneratedMods, TagEvidence};

/// Builds one [`TagEvidence`] per scanned mod, **excluding every
/// Rimmerge-generated mod** (see [`GeneratedMods`]): a generated mod lives
/// under `Mods/` like any other install, so it's `Source::Local` — exactly
/// the (weak) signal the once-shipped tag rule's
/// `SourceIs(Source::Local)` matches — and if it ever earned that tag it
/// could also be pulled into that rule's own contiguous cluster. Excluding
/// it from evidence entirely means no rule, present or future, can ever
/// tag it from inference; see [`super::infer::infer_tags`]'s own defensive
/// exclusion of its *output* for the same reason.
///
/// `def_type_namespaces` and `def_name_prefixes` come from each mod's own
/// [`ScannedMod::defs`] — the analyzer's [`Report`] only carries
/// aggregated counts (`mods_with_defs`, `total_defs_indexed`), not
/// per-mod def keys, so the raw scan output is the only source for
/// these. `assembly_refs_to` instead comes from `report`'s already
/// resolved `AssemblyRef` edges: an assembly *name* only becomes a mod
/// *id* once the analyzer has matched it against every active mod's
/// shipped assemblies, which `ScannedMod` alone can't do.
#[must_use]
pub fn collect_evidence(scanned_mods: &[ScannedMod], report: &Report) -> Vec<TagEvidence> {
    let generated = GeneratedMods::from_report(report);
    scanned_mods
        .iter()
        .filter(|scanned| !generated.contains(&scanned.info.id))
        .map(|scanned| build_one(scanned, report))
        .collect()
}

fn build_one(scanned: &ScannedMod, report: &Report) -> TagEvidence {
    let mod_id = scanned.info.id.clone();

    let assembly_refs_to = report
        .edges
        .iter()
        .filter(|edge_report| {
            edge_report.edge.kind == EdgeKind::AssemblyRef && edge_report.edge.after == mod_id
        })
        .map(|edge_report| edge_report.edge.before.base())
        .collect();

    let declared = &scanned.info.declared;
    let declares_after = declared
        .load_after
        .iter()
        .chain(declared.force_load_after.iter())
        .map(|id| id.base())
        .chain(
            declared
                .dependencies
                .iter()
                .map(|dependency| dependency.id.base()),
        )
        .collect();

    let def_type_namespaces = scanned
        .defs
        .iter()
        .map(|def| namespace_of(&def.def_type))
        .collect();
    let def_name_prefixes = scanned
        .defs
        .iter()
        .map(|def| prefix_of(&def.def_name))
        .collect();

    TagEvidence {
        mod_id,
        url: scanned.info.url.clone(),
        source: scanned.info.source,
        def_type_namespaces,
        def_name_prefixes,
        assembly_refs_to,
        declares_after,
    }
}

/// Builds one [`TagEvidence`] per mod straight from a [`Report`], with no
/// [`ScannedMod`] data available. Excludes every generated mod, same as
/// [`collect_evidence`] — see that function's doc comment for why.
///
/// Use this when only the analyzer's cached `Report` was kept (the common
/// case once a scan is done and serialized — re-scanning just to get
/// evidence back would be wasteful); use [`collect_evidence`] instead
/// whenever the original scan output is still at hand, since a `Report`
/// alone cannot populate two of `TagEvidence`'s fields:
///
/// - `def_type_namespaces` and `def_name_prefixes` need each mod's own
///   [`ScannedMod::defs`] — `Report` only carries aggregated counts
///   (`mods_with_defs`, `total_defs_indexed`), never per-mod def keys — so
///   both always come back empty here.
///
/// `url`, `source`, `declares_after` (from `DeclaredOrder`), and
/// `assembly_refs_to` (from the report's already-resolved `AssemblyRef`
/// edges) all carry over exactly as [`collect_evidence`] would produce
/// them, since `Report::mods` already carries that data.
#[must_use]
pub fn evidence_from_report(report: &Report) -> Vec<TagEvidence> {
    let generated = GeneratedMods::from_report(report);
    report
        .mods
        .iter()
        .filter(|mod_entry| !generated.contains(&mod_entry.id))
        .map(|mod_entry| build_one_from_mod(mod_entry, report))
        .collect()
}

fn build_one_from_mod(mod_entry: &rim_analyzer::domain::Mod, report: &Report) -> TagEvidence {
    let mod_id = &mod_entry.id;

    let assembly_refs_to = report
        .edges
        .iter()
        .filter(|edge_report| {
            edge_report.edge.kind == EdgeKind::AssemblyRef && edge_report.edge.after == *mod_id
        })
        .map(|edge_report| edge_report.edge.before.base())
        .collect();

    let declared = &mod_entry.declared;
    let declares_after = declared
        .load_after
        .iter()
        .chain(declared.force_load_after.iter())
        .map(|id| id.base())
        .chain(
            declared
                .dependencies
                .iter()
                .map(|dependency| dependency.id.base()),
        )
        .collect();

    TagEvidence {
        mod_id: mod_id.clone(),
        url: mod_entry.url.clone(),
        source: mod_entry.source,
        def_type_namespaces: BTreeSet::new(),
        def_name_prefixes: BTreeSet::new(),
        assembly_refs_to,
        declares_after,
    }
}

/// The namespace token before the first `.` in a def type tag
/// (`"example.PartDef"` -> `"example"`), lowercased; the whole (lowercased)
/// string when there is no `.`.
fn namespace_of(def_type: &str) -> String {
    def_type
        .split_once('.')
        .map_or(def_type, |(namespace, _)| namespace)
        .to_lowercase()
}

/// The token before the first `_` or `.` in a def name
/// (`"Example_Xxx"` -> `"example"`), lowercased; the whole (lowercased) string
/// when neither separator appears.
fn prefix_of(def_name: &str) -> String {
    match def_name.find(['_', '.']) {
        Some(index) => def_name[..index].to_lowercase(),
        None => def_name.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_analyzer::domain::{
        DeclaredOrder, DefEntry, Edge, EdgeReport, EdgeStatus, Mod, ModDependency, ModId,
        REPORT_SCHEMA_VERSION, Report, ReportMetadata, ScannedMod, Source, XmlLocator,
    };
    use std::path::Path;
    use std::sync::Arc;

    use super::*;

    fn mod_info(id: &str, url: Option<&str>, source: Source) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            url: url.map(str::to_string),
            path: std::path::PathBuf::new(),
            source,
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

    fn scanned(info: Mod, defs: Vec<DefEntry>) -> ScannedMod {
        ScannedMod {
            info,
            defs,
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
            scan_cost: rim_analyzer::domain::ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    fn def(def_type: &str, def_name: &str) -> DefEntry {
        DefEntry {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: XmlLocator::new(Arc::from(Path::new("test.xml")), vec![0]),
        }
    }

    fn empty_report() -> Report {
        Report {
            metadata: ReportMetadata {
                schema_version: REPORT_SCHEMA_VERSION,
                game_dir: std::path::PathBuf::new(),
                workshop_dir: std::path::PathBuf::new(),
                mods_config: std::path::PathBuf::new(),
                game_version: "1.6".to_string(),
                generated_at: String::new(),
                active_mod_count: 0,
                scanned_mod_count: 0,
                mods_with_assemblies: 0,
                mods_with_patches: 0,
                mods_with_defs: 0,
                total_defs_indexed: 0,
                distinct_texture_paths: 0,
                discovered_mod_count: 0,
                core_resource_texture_count: 0,
            },
            mods: Vec::new(),
            edges: Vec::new(),
            conflicts: Vec::new(),
            constraints: Vec::new(),
            undeclared_hard_dependencies: Vec::new(),
            missing_mods: Vec::new(),
            missing_dependencies: Vec::new(),
            incompatible_active_pairs: Vec::new(),
            unsupported_version_mods: Vec::new(),
            unresolved_find_mod_names: Vec::new(),
            find_mod_names_using_package_id: Vec::new(),
            warnings: Vec::new(),
            mod_costs: Vec::new(),
            inactive_mods: Vec::new(),
        }
    }

    #[test]
    fn namespace_of_splits_on_the_first_dot() {
        assert_eq!(namespace_of("example.PartDef"), "example");
        assert_eq!(namespace_of("ThingDef"), "thingdef");
    }

    #[test]
    fn prefix_of_splits_on_the_first_underscore_or_dot() {
        assert_eq!(prefix_of("Example_Xxx"), "example");
        assert_eq!(prefix_of("example.xxx"), "example");
        assert_eq!(prefix_of("PlainName"), "plainname");
    }

    #[test]
    fn collects_url_source_and_def_evidence_from_the_scanned_mod() {
        let info = mod_info(
            "example.addon",
            Some("https://mods.example/x"),
            Source::Local,
        );
        let scanned_mod = scanned(info, vec![def("example.QuirkDef", "Example_SomeQuirk")]);
        let report = empty_report();

        let evidence = collect_evidence(std::slice::from_ref(&scanned_mod), &report);

        assert_eq!(evidence.len(), 1);
        let one = &evidence[0];
        assert_eq!(one.mod_id, ModId::new("example.addon"));
        assert_eq!(one.url.as_deref(), Some("https://mods.example/x"));
        assert_eq!(one.source, Source::Local);
        assert!(one.def_type_namespaces.contains("example"));
        assert!(one.def_name_prefixes.contains("example"));
    }

    #[test]
    fn collects_assembly_refs_to_from_the_reports_resolved_edges() {
        let info = mod_info("addon", None, Source::Local);
        let scanned_mod = scanned(info, Vec::new());
        let mut report = empty_report();
        report.edges.push(EdgeReport {
            edge: Edge {
                after: ModId::new("addon"),
                before: ModId::new("example.framework"),
                kind: EdgeKind::AssemblyRef,
                detail: "ExampleFramework.dll".to_string(),
                load_time: true,
                subject: None,
            },
            status: EdgeStatus::Satisfied,
        });
        // An edge pointing the other way, or of a different kind, must
        // not contribute.
        report.edges.push(EdgeReport {
            edge: Edge {
                after: ModId::new("someone.else"),
                before: ModId::new("addon"),
                kind: EdgeKind::AssemblyRef,
                detail: String::new(),
                load_time: true,
                subject: None,
            },
            status: EdgeStatus::Satisfied,
        });

        let evidence = collect_evidence(std::slice::from_ref(&scanned_mod), &report);

        assert_eq!(evidence.len(), 1);
        assert!(
            evidence[0]
                .assembly_refs_to
                .contains(&ModId::new("example.framework"))
        );
        assert_eq!(evidence[0].assembly_refs_to.len(), 1);
    }

    #[test]
    fn collects_declares_after_from_load_after_and_dependencies() {
        let mut info = mod_info("addon", None, Source::Local);
        info.declared = DeclaredOrder {
            load_after: vec![ModId::new("framework_steam")],
            dependencies: vec![ModDependency {
                id: ModId::new("other.dep"),
                display_name: None,
            }],
            ..DeclaredOrder::default()
        };
        let scanned_mod = scanned(info, Vec::new());
        let report = empty_report();

        let evidence = collect_evidence(std::slice::from_ref(&scanned_mod), &report);

        // `_steam` suffixes are stripped so declared relations always
        // compare against base ids.
        assert!(
            evidence[0]
                .declares_after
                .contains(&ModId::new("framework"))
        );
        assert!(
            evidence[0]
                .declares_after
                .contains(&ModId::new("other.dep"))
        );
    }

    #[test]
    fn evidence_from_report_leaves_def_fields_empty_but_matches_collect_evidence_otherwise() {
        let mut info = mod_info("addon", Some("https://mods.example/x"), Source::Local);
        info.declared = DeclaredOrder {
            load_after: vec![ModId::new("framework_steam")],
            dependencies: vec![ModDependency {
                id: ModId::new("other.dep"),
                display_name: None,
            }],
            ..DeclaredOrder::default()
        };
        let scanned_mod = scanned(
            info.clone(),
            vec![def("example.QuirkDef", "Example_SomeQuirk")],
        );
        let mut report = empty_report();
        report.mods.push(info);
        report.edges.push(EdgeReport {
            edge: Edge {
                after: ModId::new("addon"),
                before: ModId::new("example.framework"),
                kind: EdgeKind::AssemblyRef,
                detail: "ExampleFramework.dll".to_string(),
                load_time: true,
                subject: None,
            },
            status: EdgeStatus::Satisfied,
        });

        let from_scan = collect_evidence(std::slice::from_ref(&scanned_mod), &report);
        let from_report = evidence_from_report(&report);

        assert_eq!(from_report.len(), 1);
        let one = &from_report[0];
        assert_eq!(one.mod_id, from_scan[0].mod_id);
        assert_eq!(one.url, from_scan[0].url);
        assert_eq!(one.source, from_scan[0].source);
        assert_eq!(one.assembly_refs_to, from_scan[0].assembly_refs_to);
        assert_eq!(one.declares_after, from_scan[0].declares_after);

        // The two signals only `ScannedMod::defs` can populate: `Report`
        // has no per-mod def data, so these stay empty even though the
        // scan-based evidence for the same mod has them.
        assert!(one.def_type_namespaces.is_empty());
        assert!(one.def_name_prefixes.is_empty());
        assert!(!from_scan[0].def_type_namespaces.is_empty());
    }

    #[test]
    fn evidence_from_report_visits_every_mod_in_report_order() {
        let mut report = empty_report();
        report.mods.push(mod_info("a", None, Source::Local));
        report.mods.push(mod_info("b", None, Source::Core));

        let evidence = evidence_from_report(&report);

        assert_eq!(
            evidence
                .iter()
                .map(|e| e.mod_id.clone())
                .collect::<Vec<_>>(),
            vec![ModId::new("a"), ModId::new("b")]
        );
    }

    /// The generated merge mod must never get a [`TagEvidence`] entry at
    /// all — not filtered after the fact, excluded outright — so no rule,
    /// present or future, can ever match it. See [`collect_evidence`]'s
    /// own doc comment for why (`Source::Local` + the once-shipped tag
    /// rule's weak `SourceIs` signal).
    #[test]
    fn collect_evidence_excludes_the_generated_merge_mod() {
        let generated_info = mod_info("rimmerge.merge.abc123456789", None, Source::Local);
        let real_info = mod_info("addon", None, Source::Local);
        let scanned_mods = vec![
            scanned(generated_info.clone(), Vec::new()),
            scanned(real_info.clone(), Vec::new()),
        ];
        // `GeneratedMods::from_report` reads `report.mods`, so — exactly as
        // in real usage (`rim-io`'s scanner builds `report` from this same
        // `scan_output.scanned_mods`) — the generated mod must appear there
        // too for the `rimmerge.merge.` prefix fallback (no marker set
        // here) to match it.
        let mut report = empty_report();
        report.mods.push(generated_info);
        report.mods.push(real_info);

        let evidence = collect_evidence(&scanned_mods, &report);

        assert_eq!(
            evidence
                .iter()
                .map(|e| e.mod_id.clone())
                .collect::<Vec<_>>(),
            vec![ModId::new("addon")],
            "the generated merge mod must never appear in the collected evidence"
        );
    }

    /// Same exclusion, for the `Report`-only path.
    #[test]
    fn evidence_from_report_excludes_the_generated_merge_mod() {
        let mut report = empty_report();
        report
            .mods
            .push(mod_info("rimmerge.merge.abc123456789", None, Source::Local));
        report.mods.push(mod_info("addon", None, Source::Local));

        let evidence = evidence_from_report(&report);

        assert_eq!(
            evidence
                .iter()
                .map(|e| e.mod_id.clone())
                .collect::<Vec<_>>(),
            vec![ModId::new("addon")],
            "the generated merge mod must never appear in the collected evidence"
        );
    }
}
