//! Computes each mod's "framework candidate" score: how many other active
//! mods declare a hard, or merely aware, relationship pointing at it.

use std::collections::{HashMap, HashSet};

use crate::domain::{EdgeReport, EdgeStrength, Mod, ModId};

/// A mod earning this many distinct Hard-strength dependents is very
/// likely a shared framework other mods build on, not a leaf content mod.
const FRAMEWORK_HARD_DEPENDENT_THRESHOLD: usize = 3;

/// Sets `hard_dependents`, `awareness_dependents`, and
/// `is_framework_candidate` on every mod, computed from the edges
/// pointing at it.
///
/// An `Inferred`-strength edge
/// (`PatchRemovedNode`/`RetextureAfterOwner`/`DefOverrideAfterOrigin`)
/// contributes to none of the three `Hard`/`Soft`/`Awareness` counts below —
/// a stated decision, not a silent omission. A heuristic edge is evidence the
/// analyzer inferred, not something an author declared or a DLL reference
/// proves, so counting it toward "framework candidate" scoring (which only
/// ever means "other mods have a real, provable dependency on this one")
/// would conflate the two; `mod_entry::Mod` has no `inferred_dependents`
/// field either, so this is silent by construction rather than an oversight
/// to fix by adding a fourth bucket.
pub fn apply(mods: &mut [Mod], edge_reports: &[EdgeReport]) {
    let hard = dependents_by_strength(edge_reports, EdgeStrength::Hard);
    let soft = dependents_by_strength(edge_reports, EdgeStrength::Soft);
    let awareness = dependents_by_strength(edge_reports, EdgeStrength::Awareness);

    for m in mods.iter_mut() {
        let hard_dependents = hard.get(&m.id).map_or(0, HashSet::len);
        m.hard_dependents = hard_dependents;
        m.soft_dependents = soft.get(&m.id).map_or(0, HashSet::len);
        m.awareness_dependents = awareness.get(&m.id).map_or(0, HashSet::len);
        m.is_framework_candidate = hard_dependents >= FRAMEWORK_HARD_DEPENDENT_THRESHOLD;
    }
}

/// For each `before` mod, the distinct set of `after` mods pointing at it
/// with an edge of exactly `strength`.
fn dependents_by_strength(
    edge_reports: &[EdgeReport],
    strength: EdgeStrength,
) -> HashMap<ModId, HashSet<ModId>> {
    let mut map: HashMap<ModId, HashSet<ModId>> = HashMap::new();
    for report in edge_reports {
        if report.edge.strength() != strength {
            continue;
        }
        map.entry(report.edge.before.clone())
            .or_default()
            .insert(report.edge.after.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeclaredOrder, Edge, EdgeKind, EdgeStatus, Source};
    use std::path::PathBuf;

    fn bare_mod(id: &str) -> Mod {
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

    /// Builds an edge report with `load_time: true`, so an `AssemblyRef`
    /// kind carries `Hard` strength by default — use [`lazy_edge_report`]
    /// to build a `Soft` one instead.
    fn edge_report(after: &str, before: &str, kind: EdgeKind) -> EdgeReport {
        edge_report_with_load_time(after, before, kind, true)
    }

    fn lazy_edge_report(after: &str, before: &str) -> EdgeReport {
        edge_report_with_load_time(after, before, EdgeKind::AssemblyRef, false)
    }

    fn edge_report_with_load_time(
        after: &str,
        before: &str,
        kind: EdgeKind,
        load_time: bool,
    ) -> EdgeReport {
        EdgeReport {
            edge: Edge {
                after: ModId::new(after),
                before: ModId::new(before),
                kind,
                detail: String::new(),
                load_time,
                subject: None,
            },
            status: EdgeStatus::Satisfied,
        }
    }

    #[test]
    fn counts_distinct_hard_dependents_and_flags_framework_candidate() {
        let mut mods = vec![bare_mod("framework")];
        let edges = vec![
            edge_report("a", "framework", EdgeKind::AssemblyRef),
            edge_report("b", "framework", EdgeKind::AssemblyRef),
            edge_report("c", "framework", EdgeKind::AssemblyRef),
            // Duplicate edge from the same mod must not double-count.
            edge_report("a", "framework", EdgeKind::ForceLoadAfter),
        ];

        apply(&mut mods, &edges);

        assert_eq!(mods[0].hard_dependents, 3);
        assert!(mods[0].is_framework_candidate);
    }

    #[test]
    fn below_threshold_is_not_a_framework_candidate() {
        let mut mods = vec![bare_mod("leaf")];
        let edges = vec![
            edge_report("a", "leaf", EdgeKind::AssemblyRef),
            edge_report("b", "leaf", EdgeKind::AssemblyRef),
        ];

        apply(&mut mods, &edges);

        assert_eq!(mods[0].hard_dependents, 2);
        assert!(!mods[0].is_framework_candidate);
    }

    #[test]
    fn counts_awareness_dependents_separately_from_hard() {
        let mut mods = vec![bare_mod("m")];
        let edges = vec![
            edge_report("a", "m", EdgeKind::AssemblyRef),
            edge_report("b", "m", EdgeKind::FindMod),
        ];

        apply(&mut mods, &edges);

        assert_eq!(mods[0].hard_dependents, 1);
        assert_eq!(mods[0].awareness_dependents, 1);
    }

    #[test]
    fn counts_soft_dependents_separately_from_hard() {
        let mut mods = vec![bare_mod("m")];
        let edges = vec![
            edge_report("a", "m", EdgeKind::AssemblyRef),
            lazy_edge_report("b", "m"),
        ];

        apply(&mut mods, &edges);

        assert_eq!(mods[0].hard_dependents, 1);
        assert_eq!(mods[0].soft_dependents, 1);
    }
}
