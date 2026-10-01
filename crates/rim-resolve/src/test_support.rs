//! `ReportBuilder`: a fluent builder for hand-made, minimal
//! [`rim_analyzer::domain::Report`]s, so sorter/ledger tests never have to
//! spell out every field of `Mod`/`Edge`/`Constraint` by hand.
//!
//! Gated behind the `test-support` feature so both this crate's own unit
//! tests (which enable it implicitly through `dev-dependencies` in
//! `Cargo.toml` re-listing this crate with the feature on) and its
//! `tests/` integration tests can share one implementation.

use std::path::PathBuf;

use rim_analyzer::domain::{
    Conflict, Constraint, ConstraintStatus, DeclaredOrder, DefOverride, Edge, EdgeKind, EdgeReport,
    EdgeStatus, InactiveMod, IncompatiblePair, LoadOrder, MissingDependency, Mod, ModCost,
    ModDependency, ModId, PatchCollision, PatchCollisionEntry, PatchCollisionSeverity,
    REPORT_SCHEMA_VERSION, Report, ReportMetadata, Selector, Source,
};

/// Builds a [`Report`] one mod/edge/constraint at a time. Every mod
/// defaults to [`Source::Local`], zero dependents, and not a framework
/// candidate; use the typed helpers ([`ReportBuilder::core`],
/// [`ReportBuilder::framework`], ...) to override that.
#[derive(Debug, Default, Clone)]
pub struct ReportBuilder {
    mods: Vec<Mod>,
    edges: Vec<EdgeReport>,
    constraints: Vec<Constraint>,
    conflicts: Vec<Conflict>,
    missing_mods: Vec<ModId>,
    incompatible_pairs: Vec<IncompatiblePair>,
    missing_dependencies: Vec<MissingDependency>,
    mod_costs: Vec<ModCost>,
    inactive_mods: Vec<InactiveMod>,
}

fn plain_mod(id: &str, source: Source) -> Mod {
    Mod {
        id: ModId::new(id),
        name: id.to_string(),
        authors: Vec::new(),
        url: None,
        path: PathBuf::new(),
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

impl ReportBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a plain `Source::Local` mod with no special standing.
    #[must_use]
    pub fn mod_(self, id: &str) -> Self {
        self.mod_with(id, |_| {})
    }

    /// Adds a mod, letting the caller adjust any field after the default
    /// is built.
    #[must_use]
    pub fn mod_with(mut self, id: &str, configure: impl FnOnce(&mut Mod)) -> Self {
        let mut entry = plain_mod(id, Source::Local);
        configure(&mut entry);
        self.mods.push(entry);
        self
    }

    /// Adds `Source::Core` vanilla content.
    #[must_use]
    pub fn core(self, id: &str) -> Self {
        self.mod_with(id, |m| m.source = Source::Core)
    }

    /// Adds `Source::Dlc` content.
    #[must_use]
    pub fn dlc(self, id: &str) -> Self {
        self.mod_with(id, |m| m.source = Source::Dlc)
    }

    /// Adds a mod flagged as a framework candidate, with the given
    /// `hard_dependents` count (the analyzer's own scoring basis).
    #[must_use]
    pub fn framework(self, id: &str, hard_dependents: usize) -> Self {
        self.mod_with(id, |m| {
            m.is_framework_candidate = true;
            m.hard_dependents = hard_dependents;
        })
    }

    /// Sets `hard_dependents` on an already-added mod (by id).
    ///
    /// # Panics
    ///
    /// Panics if no mod with `id` was added yet — a test-only helper, so a
    /// panic here means the test itself is wrong.
    #[must_use]
    pub fn hard_dependents(mut self, id: &str, count: usize) -> Self {
        let target = ModId::new(id);
        let entry = self
            .mods
            .iter_mut()
            .find(|m| m.id == target)
            .unwrap_or_else(|| panic!("no mod {id:?} added yet"));
        entry.hard_dependents = count;
        self
    }

    /// Declares `after` -> `loadAfter` -> `before` on `after`'s
    /// `DeclaredOrder` (`About.xml`-style hint), independent of whether an
    /// engine edge is also added.
    ///
    /// # Panics
    ///
    /// Panics if no mod with id `after` was added yet.
    #[must_use]
    pub fn declares_load_after(mut self, after: &str, before: &str) -> Self {
        let target = ModId::new(after);
        let entry = self
            .mods
            .iter_mut()
            .find(|m| m.id == target)
            .unwrap_or_else(|| panic!("no mod {after:?} added yet"));
        entry.declared.load_after.push(ModId::new(before));
        self
    }

    /// Adds a raw engine edge: `after` must load after `before`.
    #[must_use]
    pub fn edge(mut self, after: &str, before: &str, kind: EdgeKind, load_time: bool) -> Self {
        self.edges.push(EdgeReport {
            edge: Edge {
                after: ModId::new(after),
                before: ModId::new(before),
                kind,
                detail: format!("{kind:?}"),
                load_time,
                subject: None,
            },
            status: EdgeStatus::Unevaluated,
        });
        self
    }

    /// A `Hard`-strength edge: a load-time `AssemblyRef`.
    #[must_use]
    pub fn hard_edge(self, after: &str, before: &str) -> Self {
        self.edge(after, before, EdgeKind::AssemblyRef, true)
    }

    /// A `Declared`-strength edge (`loadAfter`).
    #[must_use]
    pub fn declared_edge(self, after: &str, before: &str) -> Self {
        self.edge(after, before, EdgeKind::LoadAfter, true)
    }

    /// A `Soft`-strength edge: a lazily-resolved `AssemblyRef`.
    #[must_use]
    pub fn soft_edge(self, after: &str, before: &str) -> Self {
        self.edge(after, before, EdgeKind::AssemblyRef, false)
    }

    /// An `Awareness`-strength edge (`MayRequire`).
    #[must_use]
    pub fn awareness_edge(self, after: &str, before: &str) -> Self {
        self.edge(after, before, EdgeKind::MayRequire, true)
    }

    /// A `ForceLoadAfter` (always-`Hard`) edge.
    #[must_use]
    pub fn force_load_after_edge(self, after: &str, before: &str) -> Self {
        self.edge(after, before, EdgeKind::ForceLoadAfter, true)
    }

    /// An any-of constraint: `after` is satisfied when any one of
    /// `candidates` loads before it.
    #[must_use]
    pub fn any_of(
        mut self,
        after: &str,
        assembly: &str,
        candidates: &[&str],
        load_time: bool,
    ) -> Self {
        self.constraints.push(Constraint::AnyOf {
            after: ModId::new(after),
            assembly: assembly.to_string(),
            candidates: candidates.iter().map(|id| ModId::new(*id)).collect(),
            load_time,
            status: ConstraintStatus::Violated,
        });
        self
    }

    /// Marks `id` as active-but-missing (not present in `report.mods`).
    #[must_use]
    pub fn missing_mod(mut self, id: &str) -> Self {
        self.missing_mods.push(ModId::new(id));
        self
    }

    /// Adds `id` to `report.inactive_mods` — a mod discovery found on
    /// disk but that isn't active. Source defaults to [`Source::Local`]
    /// and every other field to empty/`None`, matching `mod_`'s own
    /// defaults for an active mod.
    #[must_use]
    pub fn inactive(mut self, id: &str) -> Self {
        self.inactive_mods.push(InactiveMod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            path: PathBuf::new(),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        });
        self
    }

    /// Adds a `Conflict::DefOverride` naming `def_type`/`def_name`, owned
    /// by `owners` in the order given (the last owner is the winner under that order) —
    /// for tests that need a live `DefOverride` finding to plan or decide
    /// a merge over, which the plain `mod_`/`core`/... builders alone
    /// never produce (they add mods but never a conflict between them).
    ///
    /// # Panics
    ///
    /// Panics if `owners` is empty.
    #[must_use]
    pub fn def_override(mut self, def_type: &str, def_name: &str, owners: &[&str]) -> Self {
        let owners: Vec<ModId> = owners.iter().map(|id| ModId::new(*id)).collect();
        assert!(!owners.is_empty(), "def_override needs at least one owner");
        self.conflicts.push(Conflict::DefOverride(DefOverride {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            owners,
            overrides_vanilla: false,
            same_author: false,
        }));
        self
    }

    /// Adds a `Conflict::PatchCollision` naming `def_type`/`def_name`,
    /// contributed to by `mods` in load order (each a placeholder
    /// `PatchOperationReplace`, `Contested` severity) — for tests that need
    /// a live `PatchCollision` finding without spelling out every
    /// `PatchCollisionEntry` by hand.
    ///
    /// # Panics
    ///
    /// Panics if `mods` is empty.
    #[must_use]
    pub fn patch_collision(mut self, def_type: &str, def_name: &str, mods: &[&str]) -> Self {
        assert!(
            !mods.is_empty(),
            "patch_collision needs at least one contributing mod"
        );
        self.conflicts
            .push(Conflict::PatchCollision(PatchCollision {
                def_type: def_type.to_string(),
                def_name: def_name.to_string(),
                selector: Selector::DefName,
                sub_path: None,
                mods: mods
                    .iter()
                    .map(|id| PatchCollisionEntry {
                        mod_id: ModId::new(*id),
                        op_class: "PatchOperationReplace".to_string(),
                    })
                    .collect(),
                severity: PatchCollisionSeverity::Contested,
                removed_by: Vec::new(),
            }));
        self
    }

    /// Adds a `MissingDependency`: active mod `mod_id` declares `dependency`
    /// (with `display_name`) and it is not active.
    #[must_use]
    pub fn missing_dependency(
        mut self,
        mod_id: &str,
        dependency: &str,
        display_name: Option<&str>,
    ) -> Self {
        self.missing_dependencies.push(MissingDependency {
            mod_id: ModId::new(mod_id),
            dependency: ModDependency {
                id: ModId::new(dependency),
                display_name: display_name.map(str::to_string),
            },
        });
        self
    }

    /// Adds an `IncompatiblePair` between `a` and `b` — a pair-shaped
    /// finding, for tests exercising `PatchScope::membership`'s pair rule
    /// against a full pipeline-built ledger.
    #[must_use]
    pub fn incompatible_pair(mut self, a: &str, b: &str) -> Self {
        self.incompatible_pairs.push(IncompatiblePair {
            a: ModId::new(a),
            b: ModId::new(b),
        });
        self
    }

    /// A declared dependency, purely for round-tripping through
    /// [`DeclaredOrder::dependencies`] when a test needs it.
    #[must_use]
    pub fn dependency(mut self, after: &str, on: &str) -> Self {
        let target = ModId::new(after);
        if let Some(entry) = self.mods.iter_mut().find(|m| m.id == target) {
            entry.declared.dependencies.push(ModDependency {
                id: ModId::new(on),
                display_name: None,
            });
        }
        self
    }

    /// [`Self::dependency`]'s inactive-mod sibling: declares a
    /// `modDependencies` entry
    /// on an already-[`Self::inactive`] mod, for testing
    /// `ActivateMods`'s own dependency-closure walk over
    /// `Report.inactive_mods`.
    #[must_use]
    pub fn inactive_dependency(mut self, after: &str, on: &str) -> Self {
        let target = ModId::new(after);
        if let Some(entry) = self.inactive_mods.iter_mut().find(|m| m.id == target) {
            entry.declared.dependencies.push(ModDependency {
                id: ModId::new(on),
                display_name: None,
            });
        }
        self
    }

    /// Every mod id added so far, in insertion order — a convenient
    /// default `current` [`LoadOrder`] for tests that don't care about
    /// tie-breaks.
    #[must_use]
    pub fn mod_ids(&self) -> Vec<ModId> {
        self.mods.iter().map(|m| m.id.clone()).collect()
    }

    /// A [`LoadOrder`] over every mod added so far, in insertion order.
    #[must_use]
    pub fn insertion_order(&self) -> LoadOrder {
        LoadOrder::new(self.mod_ids())
    }

    /// Sets [`Report::mod_costs`] outright, replacing whatever was there
    /// (always empty until this is called), for tests that need non-empty
    /// rows (such as the `ContributesNothing` producer's); every other test
    /// gets `build()`'s `mod_costs: []` default unless it opts in here.
    #[must_use]
    pub fn with_mod_costs(mut self, mod_costs: Vec<ModCost>) -> Self {
        self.mod_costs = mod_costs;
        self
    }

    /// Builds the final [`Report`].
    #[must_use]
    pub fn build(self) -> Report {
        Report {
            metadata: ReportMetadata {
                schema_version: REPORT_SCHEMA_VERSION,
                game_dir: PathBuf::new(),
                workshop_dir: PathBuf::new(),
                mods_config: PathBuf::new(),
                game_version: "1.6".to_string(),
                generated_at: String::new(),
                active_mod_count: self.mods.len() + self.missing_mods.len(),
                scanned_mod_count: self.mods.len(),
                mods_with_assemblies: 0,
                mods_with_patches: 0,
                mods_with_defs: 0,
                total_defs_indexed: 0,
                distinct_texture_paths: 0,
                discovered_mod_count: self.mods.len() + self.inactive_mods.len(),
                core_resource_texture_count: 0,
            },
            mods: self.mods,
            edges: self.edges,
            conflicts: self.conflicts,
            constraints: self.constraints,
            undeclared_hard_dependencies: Vec::new(),
            missing_mods: self.missing_mods,
            missing_dependencies: self.missing_dependencies,
            incompatible_active_pairs: self.incompatible_pairs,
            unsupported_version_mods: Vec::new(),
            unresolved_find_mod_names: Vec::new(),
            find_mod_names_using_package_id: Vec::new(),
            warnings: Vec::new(),
            mod_costs: self.mod_costs,
            inactive_mods: self.inactive_mods,
        }
    }
}

/// Thin re-export of [`crate::tags::evidence_from_report`], kept under its
/// original name here since existing tests already call
/// `test_support::evidence_from_report`. See that function's doc comment
/// for exactly which [`crate::domain::TagEvidence`] fields it can and
/// cannot populate from a bare [`Report`].
pub use crate::tags::evidence_from_report;
