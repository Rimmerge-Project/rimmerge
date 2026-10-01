//! The patch maker's effective reference-set closure
//! and the session-backed
//! [`KnownDefs`] view [`rim_resolve::domain::AssignmentProject::set_row`]
//! validates item slots and `defName` collisions against.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{Mod, ModId, Report};
use rim_resolve::domain::{AssignmentProject, KnownDefs, Section, TargetRef};

use crate::Session;

/// The selected refs plus the transitive closure of their declared
/// `modDependencies` (`Mod.declared.dependencies`), by [`ModId::base`],
/// with `Source::Core`/`Source::Dlc` members dropped — the
/// "effective reference set". Only ever grown through *active*
/// dependencies: an inactive declared dependency has no [`Mod`] entry to
/// walk into and contributes nothing, and the traversal never revisits an
/// id already in the closure. `excluded_refs` is deliberately **not**
/// applied here — the closure is always computed over the full selection,
/// and every caller subtracts its own project's `excluded_refs` from the
/// result afterward ("the closure is derived from the report on every
/// use... and a member can be excluded by the user").
#[must_use]
pub(crate) fn effective_refs(selected: &BTreeSet<ModId>, report: &Report) -> BTreeSet<ModId> {
    let by_base: BTreeMap<ModId, &Mod> = report.mods.iter().map(|m| (m.id.base(), m)).collect();

    let mut closure: BTreeSet<ModId> = BTreeSet::new();
    let mut queue: Vec<ModId> = selected.iter().map(ModId::base).collect();
    while let Some(id) = queue.pop() {
        if !closure.insert(id.clone()) {
            continue;
        }
        let Some(m) = by_base.get(&id) else { continue };
        for dep in &m.declared.dependencies {
            let dep_base = dep.id.base();
            if by_base
                .get(&dep_base)
                .is_some_and(|dep_mod| !dep_mod.source.is_vanilla())
            {
                queue.push(dep_base);
            }
        }
    }
    closure.retain(|id| by_base.get(id).is_none_or(|m| !m.source.is_vanilla()));
    closure
}

/// A [`KnownDefs`] view backed by [`Session::sources`]'s own
/// `owners_by_def`, excluding `exclude`'s own defs from the answer
/// (see [`KnownDefs`]'s
/// own doc comment): once an assignment project is exported, its own
/// generated instances are themselves active defs of the schema's def
/// type, and without this exclusion every subsequent save of an
/// already-exported row would spuriously collide with itself. `contains`
/// answers `true` only when some *other* active mod also defines the same
/// `(def_type, name)` — a genuine collision, not this project's own prior
/// export.
///
/// **`own_instances`** answers from `project`'s own
/// sections directly ("every
/// section's free-standing rows are visible to every other section's item
/// pickers as items owned by this project") — `project` must be the same
/// project `set_row`/an export validates against, so a free-standing row
/// in one section is recognized by every other section's own `ItemSlot`
/// check, including the section currently being validated.
pub(crate) struct SessionKnownDefs<'s> {
    session: &'s Session,
    exclude: ModId,
    project: &'s AssignmentProject,
}

impl<'s> SessionKnownDefs<'s> {
    /// `exclude` is normally the project's own published package id
    /// ([`rim_resolve::domain::PatchModIdentity::package_id`]) — a no-op
    /// exclusion before the project has ever been exported, since nothing
    /// is active under that id yet. `project` is the project this view's
    /// own [`KnownDefs::own_instances`] answers from — normally the same
    /// project the caller is about to validate a row/export against.
    pub(crate) fn new(
        session: &'s Session,
        exclude: ModId,
        project: &'s AssignmentProject,
    ) -> Self {
        Self {
            session,
            exclude,
            project,
        }
    }
}

impl KnownDefs for SessionKnownDefs<'_> {
    fn contains(&self, def_type: &str, name: &str) -> bool {
        self.session
            .sources()
            .owners_by_def
            .get(&(def_type.to_string(), name.to_string()))
            .is_some_and(|owners| {
                owners
                    .iter()
                    .any(|owner| owner.base() != self.exclude.base())
            })
    }

    fn own_instances(&self, def_type: &str) -> BTreeSet<String> {
        self.project
            .section(def_type)
            .map(Section::own_instance_names)
            .unwrap_or_default()
    }
}

/// The default emitted `defName` for a target's row:
/// `<folder_name>_<target def_name>`, editable, validated unique within
/// the project by [`AssignmentProject::set_row`]. Shared here (not private
/// to one use case) so every caller that needs to propose a fresh row's
/// `defName` — `CopyFrom` today, a later wizard/bulk-cover step tomorrow —
/// computes it the identical way.
#[must_use]
pub(crate) fn default_def_name(project: &AssignmentProject, target: &TargetRef) -> String {
    format!(
        "{}_{}",
        project.identity().folder_name(),
        target.def.def_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rim_resolve::test_support::ReportBuilder;

    /// A real-install regression shape: selecting only the addon (which
    /// declares a dependency on
    /// the framework) must still pull the framework into R.
    #[test]
    fn closure_follows_a_declared_dependency_transitively() {
        let report = ReportBuilder::new()
            .mod_("example.speciessupport")
            .mod_("example.framework")
            .dependency("example.speciessupport", "example.framework")
            .build();
        let selected: BTreeSet<ModId> =
            [ModId::new("example.speciessupport")].into_iter().collect();

        let closure = effective_refs(&selected, &report);

        assert_eq!(
            closure,
            [
                ModId::new("example.speciessupport"),
                ModId::new("example.framework")
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn closure_excludes_core_and_dlc_dependencies() {
        let report = ReportBuilder::new()
            .mod_("some.mod")
            .core("ludeon.rimworld")
            .dependency("some.mod", "ludeon.rimworld")
            .build();
        let selected: BTreeSet<ModId> = [ModId::new("some.mod")].into_iter().collect();

        let closure = effective_refs(&selected, &report);

        assert_eq!(closure, [ModId::new("some.mod")].into_iter().collect());
    }

    #[test]
    fn closure_ignores_an_inactive_declared_dependency() {
        let report = ReportBuilder::new()
            .mod_("some.mod")
            .dependency("some.mod", "not.active")
            .build();
        let selected: BTreeSet<ModId> = [ModId::new("some.mod")].into_iter().collect();

        let closure = effective_refs(&selected, &report);

        assert_eq!(closure, [ModId::new("some.mod")].into_iter().collect());
    }

    fn schema(def_type: &str) -> rim_resolve::domain::AssignmentSchema {
        rim_resolve::domain::AssignmentSchema {
            def_type: def_type.to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        }
    }

    fn two_section_project() -> AssignmentProject {
        AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "name")
                .expect("valid identity"),
            BTreeSet::new(),
            BTreeSet::new(),
            schema("example.PartAssignmentDef"),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    struct NoneKnown;
    impl KnownDefs for NoneKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
    }

    /// `SessionKnownDefs::own_instances` must answer
    /// from `project`'s own sections, not the session's active install —
    /// a free-standing row in one section is "known" the moment it's set,
    /// with no session/report involvement at all.
    #[test]
    fn own_instances_reads_a_free_standing_sections_own_row_names() {
        let mut project = two_section_project();
        project
            .add_section(schema("example.PartDef"))
            .expect("a fresh def type must add cleanly");
        project
            .set_row(
                "example.PartDef",
                rim_resolve::domain::RowKey::Own("MyPart".to_string()),
                rim_resolve::domain::AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "MyPart".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("a valid free-standing row");
        let session = crate::test_support::session_fixture(&["a"]);
        let known = SessionKnownDefs::new(&session, ModId::new("mypatch.parts"), &project);

        assert_eq!(
            known.own_instances("example.PartDef"),
            BTreeSet::from(["MyPart".to_string()])
        );
        assert!(
            known.own_instances("example.PartAssignmentDef").is_empty(),
            "the target-keyed section has no free-standing rows of its own"
        );
        assert!(
            known.own_instances("no.such.type").is_empty(),
            "a def type with no section at all has no own instances either"
        );
    }
}
