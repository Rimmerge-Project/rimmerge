//! [`UpdateAssignment`]: applies any of an assignment project's mutable
//! fields — name/author/description/R/T/schema confirmation — reporting a
//! [`SchemaChange`] on an R change, the row values it stranded, and the
//! rows dropped on a T shrink.
//! `rim-resolve`'s own `AssignmentProject::set_refs`/`set_targets` are
//! plain setters with no reporting (see `rim_resolve::domain::assignment`'s
//! own module doc comment) — both live here instead, since both need live
//! report data (re-inference, an owner lookup) that pure crate has no way
//! to read.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentId, FieldPath, FieldRole, FieldSpec, RowKey, RowValue, TargetRef,
};

use super::assignment_instances::{AssignmentInstances, AssignmentInstancesError};
use crate::assignment_refs::effective_refs;
use crate::ports::{AssignmentProjectStore, DefSourceReader, StoreError};
use crate::{Session, UnknownAssignment};

/// [`UpdateAssignment::reinfer_schemas`]'s own return shape — a plain type
/// alias purely to keep clippy's `type_complexity` lint happy, the same
/// reason `rim-session`'s own `AssignmentInstanceList` exists.
type ReinferOutcome = (
    BTreeMap<String, SchemaChange>,
    Vec<(String, RowKey, FieldPath)>,
);

/// Any of an assignment project's mutable fields — `None` means "leave
/// unchanged". `refs`/`excluded_refs` are re-inferred together whenever
/// either is given (the effective set needs both);
/// `targets` drops any row whose target left T.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateAssignmentInput {
    /// A new list label.
    pub name: Option<String>,
    /// A new `About.xml` author.
    pub author: Option<String>,
    /// A new `About.xml` description.
    pub description: Option<String>,
    /// A new selected reference set.
    pub refs: Option<BTreeSet<ModId>>,
    /// A new set of closure members opted out.
    pub excluded_refs: Option<BTreeSet<ModId>>,
    /// A new target set.
    pub targets: Option<BTreeSet<ModId>>,
}

/// What re-inferring the schema against a new effective R changed —
/// a `SchemaChange { added, removed, reclassified }`,
/// hosted here (not `rim-resolve`) since computing it needs a
/// fresh [`rim_resolve::domain::AssignmentSchema::infer_fields`] call
/// this pure crate has no report/instances to run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaChange {
    /// Fields the new instances/R combination observes that the schema
    /// didn't have before.
    pub added: Vec<FieldPath>,
    /// Fields the schema had that the fresh inference no longer observes
    /// at all (only possible if the underlying instances themselves
    /// changed — an R change alone never removes an observed field, only
    /// reclassifies it).
    pub removed: Vec<FieldPath>,
    /// Fields present both before and after whose *stored* role actually
    /// changed as a result of this update. Only ever a field the user
    /// never manually reclassified (`FieldSpec::inferred_role` was
    /// `None`) — a field the user already confirmed keeps its confirmed
    /// role untouched here (re-inference never silently overwrites a
    /// deliberate choice), so
    /// it can never appear in this list even when the fresh inference
    /// would classify it differently; the wizard's own diff view (a later
    /// step) is where that "confirmed role no longer matches inference"
    /// fact belongs, not a silent field-value change.
    ///
    /// A field named here may also strand an existing row's stored value:
    /// a value stored under
    /// the *old* role's shape (e.g. `RowValue::Text` for a `Scalar`) isn't
    /// automatically valid under the *new* one (e.g. `ItemSlot`, which
    /// only ever accepts `RowValue::Names`). [`UpdateAssignmentOutcome::stranded_values`]
    /// is where that gets reported — never silently, and never left in
    /// place with a shape its own new role can't render.
    pub reclassified: Vec<FieldPath>,
}

/// What [`UpdateAssignment::execute`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateAssignmentOutcome {
    /// Every section's own [`SchemaChange`], by def type — populated (one
    /// entry per section the project has, even when a section's own diff
    /// is empty) when `refs`/`excluded_refs` was part of the update,
    /// otherwise empty (the caller can tell "nothing changed" from "not
    /// requested" this way: an empty map either way, but a caller that
    /// cares tests `refs.is_some() || excluded_refs.is_some()` on its own
    /// input instead).
    pub schema_changes: BTreeMap<String, SchemaChange>,
    /// Every row value dropped because a reclassified field's new role no
    /// longer accepts the shape it was stored under (`SchemaChange::reclassified`'s
    /// own doc comment) — `(section def type, row, field path)`. Populated
    /// under the same condition as `schema_changes` (a `refs`/`excluded_refs`
    /// update); empty when nothing was stranded.
    pub stranded_values: Vec<(String, RowKey, FieldPath)>,
    /// Every row dropped because its target's owner left T, by the
    /// section (def type) it was dropped from — every target-keyed
    /// section is checked, a free-standing one ignores T entirely. Empty
    /// when `targets` wasn't part of the update, or nothing was dropped.
    pub dropped_rows: Vec<(String, TargetRef)>,
}

/// Everything that can go wrong updating an assignment project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateAssignmentError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] UnknownAssignment),
    /// Re-inferring a section's own schema failed.
    #[error(transparent)]
    Instances(#[from] AssignmentInstancesError),
    /// Persisting the update failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Applies any of [`UpdateAssignmentInput`]'s given fields to one
/// assignment project.
pub struct UpdateAssignment<Store, Reader> {
    store: Store,
    reader: Reader,
}

impl<Store: AssignmentProjectStore, Reader: DefSourceReader> UpdateAssignment<Store, Reader> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(store: Store, reader: Reader) -> Self {
        Self { store, reader }
    }

    /// # Errors
    ///
    /// See [`UpdateAssignmentError`]. On any error, `session`'s own copy
    /// of the project is restored to what it was before this call.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        input: UpdateAssignmentInput,
    ) -> Result<UpdateAssignmentOutcome, UpdateAssignmentError> {
        let snapshot = session
            .assignment_snapshot(id)
            .ok_or_else(|| UnknownAssignment(id.clone()))?;
        let mut project = snapshot.clone();

        apply_identity_fields(&mut project, input.name, input.author, input.description);

        let mut outcome = UpdateAssignmentOutcome::default();
        if input.refs.is_some() || input.excluded_refs.is_some() {
            let (schema_changes, stranded_values) =
                self.reinfer_schemas(session, &mut project, input.refs, input.excluded_refs)?;
            outcome.schema_changes = schema_changes;
            outcome.stranded_values = stranded_values;
        }
        if let Some(targets) = input.targets {
            outcome.dropped_rows = apply_targets_change(session, &mut project, targets);
        }

        self.persist(session, id, project, snapshot)?;
        Ok(outcome)
    }

    /// Re-infers *every* section's own schema against a new effective R
    /// (`refs`/`excluded_refs`, either given, defaulting to the project's
    /// own current value otherwise) and reconciles each
    /// result onto its existing schema via [`reconcile_fields`],
    /// returning one [`SchemaChange`] per section (by def type) and every
    /// row value [`drop_stranded_values`] had to strip because a
    /// reclassified field's new role no longer accepts its old shape.
    fn reinfer_schemas(
        &self,
        session: &mut Session,
        project: &mut rim_resolve::domain::AssignmentProject,
        refs: Option<BTreeSet<ModId>>,
        excluded_refs: Option<BTreeSet<ModId>>,
    ) -> Result<ReinferOutcome, UpdateAssignmentError> {
        let new_refs = refs.unwrap_or_else(|| project.refs().clone());
        let new_excluded = excluded_refs.unwrap_or_else(|| project.excluded_refs().clone());
        let effective: BTreeSet<ModId> = effective_refs(&new_refs, session.report())
            .difference(&new_excluded)
            .cloned()
            .collect();

        let def_types: Vec<String> = project.sections().keys().cloned().collect();
        let mut changes = BTreeMap::new();
        let mut stranded_values = Vec::new();
        for def_type in def_types {
            let instances = AssignmentInstances::new(&self.reader).execute(session, &def_type)?;
            let new_fields = infer_fields(session, &instances, &effective);

            let section = project
                .section(&def_type)
                .unwrap_or_else(|| unreachable!("def_type came from project.sections() above"));
            let old_fields = section.schema.fields.clone();
            let (fields, change) = reconcile_fields(&old_fields, new_fields);
            let mut schema = section.schema.clone();
            schema.refs = effective.clone();
            schema.fields = fields;
            project.set_section_schema(&def_type, schema);

            stranded_values.extend(
                drop_stranded_values(project, &def_type, &change.reclassified)
                    .into_iter()
                    .map(|(key, path)| (def_type.clone(), key, path)),
            );
            changes.insert(def_type, change);
        }
        project.set_refs(new_refs);
        project.set_excluded_refs(new_excluded);
        Ok((changes, stranded_values))
    }

    /// Persists `project` (already mutated) and makes it visible on
    /// `session`, rolling back to `snapshot` if the save fails.
    fn persist(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        project: rim_resolve::domain::AssignmentProject,
        snapshot: rim_resolve::domain::AssignmentProject,
    ) -> Result<(), UpdateAssignmentError> {
        session.upsert_assignment(project);
        let stored = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("just upserted above"));
        if let Err(error) = self.store.save(&session.paths().profile_dir, stored) {
            session.restore_assignment(snapshot);
            return Err(UpdateAssignmentError::Store(error));
        }
        Ok(())
    }
}

/// Applies the three plain-setter fields — `None` means "leave unchanged".
fn apply_identity_fields(
    project: &mut rim_resolve::domain::AssignmentProject,
    name: Option<String>,
    author: Option<String>,
    description: Option<String>,
) {
    if let Some(name) = name {
        project.set_name(name);
    }
    if let Some(author) = author {
        project.set_author(author);
    }
    if let Some(description) = description {
        project.set_description(description);
    }
}

/// Re-runs field inference over `instances` against `effective`,
/// building the `resolve`/`dll_owner`/`existing_def_type` closures
/// straight off `session.sources()` — the same construction
/// [`super::create_assignment::CreateAssignment::propose`] uses.
fn infer_fields(
    session: &Session,
    instances: &[(ModId, rim_resolve::domain::InstanceValues)],
    effective: &BTreeSet<ModId>,
) -> std::collections::BTreeMap<FieldPath, FieldSpec> {
    let sources = session.sources();
    let resolve = |value: &str| sources.defs_by_name.get(value).cloned().unwrap_or_default();
    let dll_owner = |type_name: &str| sources.dll_owner_of(type_name).cloned();
    let existing_types_lower: BTreeSet<String> = sources
        .owners_by_def
        .keys()
        .map(|(t, _)| t.to_ascii_lowercase())
        .collect();
    let existing_def_type =
        |type_name: &str| existing_types_lower.contains(&type_name.to_ascii_lowercase());
    rim_resolve::domain::AssignmentSchema::infer_fields(
        instances,
        effective,
        &resolve,
        &dll_owner,
        &existing_def_type,
    )
}

/// Mirrors `rim_resolve::domain::assignment::project`'s own private
/// `value_shape_matches` (the same "duplicate a ~10-line private helper
/// across a crate boundary rather than expose it for one caller" call
/// `def_conflict_view.rs`'s own `segment_from_step` already makes for
/// `plan_merge.rs`'s identical private helper): `Omit` always matches (an
/// intentional "use the default"), matching `set_row`'s own rule.
fn value_matches_role(role: &FieldRole, value: &RowValue) -> bool {
    match (role, value) {
        (FieldRole::TargetKey { .. }, _) => false,
        (_, RowValue::Omit) => true,
        (FieldRole::ItemSlot { .. }, RowValue::Names(_)) => true,
        (FieldRole::Chances { .. }, RowValue::Numbers(_)) => true,
        (FieldRole::Scalar { .. }, RowValue::Text(_)) => true,
        _ => false,
    }
}

/// Drops every stored value, in `def_type`'s own section, whose field is
/// named in `reclassified` and whose shape no longer matches that field's
/// *new* role — e.g. a
/// `RowValue::Text` stranded by a `Scalar` field reclassified to
/// `ItemSlot`, which only ever accepts `RowValue::Names`. Returns every
/// `(row, field path)` it stripped, so the caller can fold it into a
/// wider report the same way [`drop_rows_outside_targets`] already is.
/// A no-op when `def_type` has no section (unreachable in practice —
/// every caller here got `def_type` from `project.sections()` itself).
fn drop_stranded_values(
    project: &mut rim_resolve::domain::AssignmentProject,
    def_type: &str,
    reclassified: &[FieldPath],
) -> Vec<(RowKey, FieldPath)> {
    let Some(section) = project.section(def_type) else {
        return Vec::new();
    };
    let mut stranded = Vec::new();
    for path in reclassified {
        let Some(spec) = section.schema.fields.get(path) else {
            continue;
        };
        for (key, row) in &section.rows {
            let Some(value) = row.values.get(path) else {
                continue;
            };
            if !value_matches_role(&spec.role, value) {
                stranded.push((key.clone(), path.clone()));
            }
        }
    }
    if let Some(section) = project.section_mut(def_type) {
        for (key, path) in &stranded {
            if let Some(row) = section.rows.get_mut(key) {
                row.values.remove(path);
            }
        }
    }
    stranded
}

/// Drops every row (in every target-keyed section) whose target left the
/// new `targets` set, then replaces `project`'s own target set — a
/// free-standing section ignores `targets` entirely
///
fn apply_targets_change(
    session: &Session,
    project: &mut rim_resolve::domain::AssignmentProject,
    targets: BTreeSet<ModId>,
) -> Vec<(String, TargetRef)> {
    let dropped = drop_rows_outside_targets(session, project, &targets);
    project.set_targets(targets);
    dropped
}

/// Merges a fresh inference's field map onto the schema's current one,
/// per [`SchemaChange`]'s own doc comment: a hand-added field (`observed
/// == (0, 0)`, [`rim_resolve::domain::AssignmentSchema::add_field`]'s own
/// signature) always carries over untouched and is never "removed"; a
/// field the user never reclassified follows the fresh inference; a field
/// the user did reclassify keeps its own role untouched.
fn reconcile_fields(
    old: &std::collections::BTreeMap<FieldPath, FieldSpec>,
    mut new: std::collections::BTreeMap<FieldPath, FieldSpec>,
) -> (
    std::collections::BTreeMap<FieldPath, FieldSpec>,
    SchemaChange,
) {
    let mut change = SchemaChange::default();
    let mut fields = std::collections::BTreeMap::new();

    for (path, old_spec) in old {
        let hand_added = old_spec.observed == (0, 0);
        match new.remove(path) {
            Some(new_spec) => {
                if hand_added {
                    fields.insert(path.clone(), old_spec.clone());
                } else if old_spec.inferred_role.is_some() {
                    // The user already confirmed this field: keep their
                    // choice, never silently overwrite it.
                    fields.insert(path.clone(), old_spec.clone());
                } else {
                    if new_spec.role != old_spec.role {
                        change.reclassified.push(path.clone());
                    }
                    fields.insert(path.clone(), new_spec);
                }
            }
            None => {
                if hand_added {
                    fields.insert(path.clone(), old_spec.clone());
                } else {
                    change.removed.push(path.clone());
                }
            }
        }
    }
    for (path, spec) in new {
        change.added.push(path.clone());
        fields.insert(path, spec);
    }

    (fields, change)
}

/// Every mod owning `target`'s own `(def_type, def_name)`, by
/// [`rim_analyzer::analysis::SourceIndex::owners_by_def`].
fn target_owners(session: &Session, target: &TargetRef) -> Vec<ModId> {
    session
        .sources()
        .owners_by_def
        .get(&(target.def.def_type.clone(), target.def.def_name.clone()))
        .cloned()
        .unwrap_or_default()
}

/// Removes (and returns, per section) every target-keyed row whose
/// target's owner is no longer in `new_targets` — a target whose def has
/// since gone inactive entirely (no owner at all) is dropped too, the
/// same as one whose only owner left T. A free-standing section
/// contributes nothing (it has no `RowKey::Target` rows to begin with).
fn drop_rows_outside_targets(
    session: &Session,
    project: &mut rim_resolve::domain::AssignmentProject,
    new_targets: &BTreeSet<ModId>,
) -> Vec<(String, TargetRef)> {
    let def_types: Vec<String> = project.sections().keys().cloned().collect();
    let mut dropped = Vec::new();
    for def_type in def_types {
        let section = project
            .section(&def_type)
            .unwrap_or_else(|| unreachable!("def_type came from project.sections() above"));
        let to_drop: Vec<TargetRef> = section
            .rows
            .keys()
            .filter_map(|key| match key {
                RowKey::Target(target) => Some(target),
                RowKey::Own(_) => None,
            })
            .filter(|target| {
                !target_owners(session, target)
                    .iter()
                    .any(|owner| new_targets.contains(&owner.base()))
            })
            .cloned()
            .collect();
        for target in to_drop {
            project.clear_row(&def_type, &RowKey::Target(target.clone()));
            dropped.push((def_type.clone(), target));
        }
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryAssignmentProjectStore, session_with_sources_and_mods};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, XmlLocator};
    use rim_resolve::domain::{
        AssignmentRow, Cardinality, DefKey, FieldRole, KnownDefs, ScalarKind,
    };
    use rim_resolve::test_support::ReportBuilder;
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;

    use crate::ports::{DefSourceError, ElementExpectation};

    struct FakeReader {
        by_ordinal: BTreeMap<u32, String>,
    }

    impl DefSourceReader for FakeReader {
        fn read_element(
            &self,
            locator: &XmlLocator,
            _expected: &ElementExpectation,
        ) -> Result<String, DefSourceError> {
            self.by_ordinal
                .get(&locator.element_path[0])
                .cloned()
                .ok_or_else(|| DefSourceError::Io {
                    file: locator.file.to_path_buf(),
                    message: "not seeded".to_string(),
                })
        }
    }

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
    }

    struct NoneKnown;
    impl KnownDefs for NoneKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
    }

    /// Two `example.PartAssignmentDef` instances, each naming a race and (via a
    /// `primaryTool` slot) a part — the part is owned by `framework`
    /// alone at first (`ItemSlot`), and by `outsider` once the fixture's
    /// own resolver is swapped so a growing R changes which type wins the
    /// namespace signal... simplified here to a *plain* ownership-share
    /// swap instead: `primaryTool` starts as a `TargetKey` (its values'
    /// owner, `outsider`, isn't in R yet) and becomes an `ItemSlot` once R
    /// grows to include `outsider` (the classic "growing R reclassifies a
    /// field" case).
    fn fixture() -> (SourceIndex, FakeReader, rim_analyzer::domain::Report) {
        let mut index = SourceIndex::default();
        let mut by_ordinal = BTreeMap::new();

        for i in 0..5u32 {
            let group = format!("Group_R{i}");
            let race = format!("Race{i}");
            let part = format!("Part{i}");
            index.defs.insert(
                (
                    ModId::new("framework"),
                    ("example.PartAssignmentDef".to_string(), group.clone()),
                ),
                vec![DefEntry {
                    def_type: "example.PartAssignmentDef".to_string(),
                    def_name: group.clone(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(i),
                }],
            );
            index.owners_by_def.insert(
                ("example.PartAssignmentDef".to_string(), group.clone()),
                vec![ModId::new("framework")],
            );
            by_ordinal.insert(i,
                format!("<example.PartAssignmentDef><defName>{group}</defName><speciesNames><li>{race}</li></speciesNames><primaryTool><li>{part}</li></primaryTool></example.PartAssignmentDef>"
                ));

            index
                .defs_by_name
                .entry(race.clone())
                .or_default()
                .push(("ThingDef".to_string(), ModId::new("target.races")));
            index
                .defs_by_name
                .entry(part.clone())
                .or_default()
                .push(("example.PartDef".to_string(), ModId::new("outsider")));
        }

        let report = ReportBuilder::new()
            .mod_("framework")
            .mod_("outsider")
            .mod_("target.races")
            .build();
        (index, FakeReader { by_ordinal }, report)
    }

    #[test]
    fn growing_r_reclassifies_a_field_and_reports_it() {
        let (sources, reader, report) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report,
            &["framework", "outsider", "target.races"],
        );
        // Seed a project whose R doesn't include `outsider` yet: infer a
        // baseline schema first through the same pure engine.
        let instances = AssignmentInstances::new(&reader)
            .execute(&mut session, "example.PartAssignmentDef")
            .expect("read must succeed");
        let refs_before: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
        let fields_before = {
            let sources = session.sources();
            let resolve = |v: &str| sources.defs_by_name.get(v).cloned().unwrap_or_default();
            let dll_owner = |t: &str| sources.dll_owner_of(t).cloned();
            let no_existing_def_type = |_: &str| false;
            rim_resolve::domain::AssignmentSchema::infer_fields(
                &instances,
                &refs_before,
                &resolve,
                &dll_owner,
                &no_existing_def_type,
            )
        };
        let primary_tool: FieldPath = "primaryTool".parse().unwrap();
        assert_eq!(
            fields_before[&primary_tool].role,
            FieldRole::TargetKey {
                def_type: "example.PartDef".to_string()
            },
            "sanity: primaryTool is a TargetKey before R grows to include its owner"
        );
        let project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "name")
                .expect("valid identity"),
            refs_before,
            [ModId::new("target.races")].into_iter().collect(),
            rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartAssignmentDef".to_string(),
                refs: [ModId::new("framework")].into_iter().collect(),
                fields: fields_before,
                target_shapes: BTreeMap::new(),
            },
            jiff::Timestamp::UNIX_EPOCH,
        );
        let id = project.id().clone();
        session.upsert_assignment(project);

        let use_case = UpdateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdateAssignmentInput {
                    refs: Some(
                        [ModId::new("framework"), ModId::new("outsider")]
                            .into_iter()
                            .collect(),
                    ),
                    ..UpdateAssignmentInput::default()
                },
            )
            .expect("update must succeed");

        let change = outcome
            .schema_changes
            .get("example.PartAssignmentDef")
            .expect("refs were part of the update, so every section gets an entry");
        assert!(change.reclassified.contains(&primary_tool), "{change:?}");
        let after = session.assignment(&id).expect("still loaded");
        assert_eq!(
            after
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .schema
                .fields[&primary_tool]
                .role,
            FieldRole::ItemSlot {
                def_type: "example.PartDef".to_string()
            }
        );
    }

    #[test]
    fn shrinking_targets_drops_rows_whose_owner_left_and_reports_them() {
        let (sources, reader, report) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report,
            &["framework", "outsider", "target.races"],
        );
        // The row's own target ("Race0") is owned by nothing in
        // `owners_by_def` in this fixture (only `defs_by_name` names its
        // resolver-side owner) — seed an explicit owner entry so the drop
        // rule has something real to check.
        {
            let mut updated_sources = session.sources().clone();
            updated_sources.owners_by_def.insert(
                ("ThingDef".to_string(), "Race0".to_string()),
                vec![ModId::new("target.races")],
            );
            session = session_with_sources_and_mods(
                updated_sources,
                crate::test_support::report_fixture(&["framework", "outsider", "target.races"]),
                &["framework", "outsider", "target.races"],
            );
        }

        let mut schema_fields = BTreeMap::new();
        schema_fields.insert(
            "speciesNames".parse().unwrap(),
            FieldSpec {
                role: FieldRole::TargetKey {
                    def_type: "ThingDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        let schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields: schema_fields,
            target_shapes: BTreeMap::new(),
        };
        let mut project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "name")
                .expect("valid identity"),
            BTreeSet::new(),
            [ModId::new("target.races")].into_iter().collect(),
            schema,
            jiff::Timestamp::UNIX_EPOCH,
        );
        let target = TargetRef {
            key_field: "speciesNames".parse().unwrap(),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Race0".to_string(),
            },
        };
        project
            .set_row(
                "example.PartAssignmentDef",
                rim_resolve::domain::RowKey::Target(target.clone()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "test_Race0".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("valid row");
        let id = project.id().clone();
        session.upsert_assignment(project);

        let use_case = UpdateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdateAssignmentInput {
                    targets: Some(BTreeSet::new()),
                    ..UpdateAssignmentInput::default()
                },
            )
            .expect("update must succeed");

        assert_eq!(
            outcome.dropped_rows,
            vec![("example.PartAssignmentDef".to_string(), target)]
        );
        assert!(
            session
                .assignment(&id)
                .expect("still loaded")
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows
                .is_empty()
        );
    }

    /// Shrinking T drops target rows in every target-keyed section — a
    /// project with two
    /// independent target-keyed sections, both referencing the shared T,
    /// must have its row dropped in *both* once T shrinks, each reported
    /// under its own def type.
    #[test]
    fn shrinking_targets_drops_rows_across_two_sections() {
        let (sources, reader, report) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report,
            &["framework", "outsider", "target.races"],
        );
        {
            let mut updated_sources = session.sources().clone();
            updated_sources.owners_by_def.insert(
                ("ThingDef".to_string(), "Race0".to_string()),
                vec![ModId::new("target.races")],
            );
            session = session_with_sources_and_mods(
                updated_sources,
                crate::test_support::report_fixture(&["framework", "outsider", "target.races"]),
                &["framework", "outsider", "target.races"],
            );
        }

        let mut schema_fields = BTreeMap::new();
        schema_fields.insert(
            "speciesNames".parse().unwrap(),
            FieldSpec {
                role: FieldRole::TargetKey {
                    def_type: "ThingDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        let schema_a = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields: schema_fields.clone(),
            target_shapes: BTreeMap::new(),
        };
        let mut schema_b = schema_a.clone();
        schema_b.def_type = "other.PartAssignmentDef".to_string();
        let mut project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "name")
                .expect("valid identity"),
            BTreeSet::new(),
            [ModId::new("target.races")].into_iter().collect(),
            schema_a,
            jiff::Timestamp::UNIX_EPOCH,
        );
        project
            .add_section(schema_b)
            .expect("a fresh def type must add cleanly");
        let target = TargetRef {
            key_field: "speciesNames".parse().unwrap(),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Race0".to_string(),
            },
        };
        for def_type in ["example.PartAssignmentDef", "other.PartAssignmentDef"] {
            project
                .set_row(
                    def_type,
                    rim_resolve::domain::RowKey::Target(target.clone()),
                    AssignmentRow {
                        values: BTreeMap::new(),
                        def_name: format!("test_{def_type}_Race0"),
                        note: None,
                    },
                    &NoneKnown,
                )
                .expect("valid row");
        }
        let id = project.id().clone();
        session.upsert_assignment(project);

        let use_case = UpdateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdateAssignmentInput {
                    targets: Some(BTreeSet::new()),
                    ..UpdateAssignmentInput::default()
                },
            )
            .expect("update must succeed");

        let mut dropped = outcome.dropped_rows.clone();
        dropped.sort();
        assert_eq!(
            dropped,
            vec![
                ("example.PartAssignmentDef".to_string(), target.clone()),
                ("other.PartAssignmentDef".to_string(), target),
            ]
        );
        let after = session.assignment(&id).expect("still loaded");
        for def_type in ["example.PartAssignmentDef", "other.PartAssignmentDef"] {
            assert!(
                after
                    .section(def_type)
                    .unwrap_or_else(|| unreachable!("both sections still exist"))
                    .rows
                    .is_empty(),
                "{def_type} must have its row dropped too"
            );
        }
    }

    #[test]
    fn a_failed_save_rolls_back_every_field() {
        let (sources, reader, report) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report,
            &["framework", "outsider", "target.races"],
        );
        let project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            rim_resolve::domain::PatchModIdentity::new("mypatch.parts", "name")
                .expect("valid identity"),
            BTreeSet::new(),
            [ModId::new("target.races")].into_iter().collect(),
            rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartAssignmentDef".to_string(),
                refs: BTreeSet::new(),
                fields: BTreeMap::new(),
                target_shapes: BTreeMap::new(),
            },
            jiff::Timestamp::UNIX_EPOCH,
        );
        let id = project.id().clone();
        session.upsert_assignment(project);
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = UpdateAssignment::new(store, reader);

        let result = use_case.execute(
            &mut session,
            &id,
            UpdateAssignmentInput {
                name: Some("renamed".to_string()),
                ..UpdateAssignmentInput::default()
            },
        );

        assert!(matches!(result, Err(UpdateAssignmentError::Store(_))));
        assert_ne!(
            session.assignment(&id).map(|p| p.name().to_string()),
            Some("renamed".to_string()),
            "the rename must be rolled back when the save fails"
        );
    }

    // -- reconcile_fields: each documented branch, in isolation ----------
    //
    // `growing_r_reclassifies_a_field_and_reports_it` above only exercises
    // the "untouched field follows fresh inference" branch end to end;
    // these call `reconcile_fields` directly so each of the other three
    // documented branches has its own guard-deletion-sensitive test.

    fn spec(
        role: FieldRole,
        observed: (usize, usize),
        inferred_role: Option<FieldRole>,
    ) -> FieldSpec {
        FieldSpec {
            role,
            cardinality: Cardinality::Scalar,
            observed,
            inferred_role,
        }
    }

    #[test]
    fn a_hand_added_field_is_kept_untouched_and_never_reported_removed() {
        // observed == (0, 0) is `AssignmentSchema::add_field`'s own
        // signature for "the user added this by hand, it has no XML
        // backing" — a fresh inference naturally has no entry for it.
        let path: FieldPath = "custom".parse().unwrap();
        let hand_added = spec(FieldRole::Opaque, (0, 0), None);
        let old = BTreeMap::from([(path.clone(), hand_added.clone())]);
        let new = BTreeMap::new();

        let (fields, change) = reconcile_fields(&old, new);

        assert_eq!(
            fields.get(&path),
            Some(&hand_added),
            "a hand-added field must carry over exactly as it was"
        );
        assert!(
            change.removed.is_empty(),
            "a hand-added field must never be reported removed: {change:?}"
        );
    }

    #[test]
    fn a_user_confirmed_field_keeps_its_role_even_when_fresh_inference_disagrees() {
        let path: FieldPath = "primaryTool".parse().unwrap();
        // `inferred_role: Some(_)` is exactly what `AssignmentSchema::confirm_role`
        // sets the first time a field is reclassified — this field was
        // once inferred `Opaque` and the user overrode it to `TargetKey`.
        let confirmed = spec(
            FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            (1, 1),
            Some(FieldRole::Opaque),
        );
        let old = BTreeMap::from([(path.clone(), confirmed.clone())]);
        let freshly_inferred = spec(
            FieldRole::ItemSlot {
                def_type: "example.PartDef".to_string(),
            },
            (1, 1),
            None,
        );
        let new = BTreeMap::from([(path.clone(), freshly_inferred)]);

        let (fields, change) = reconcile_fields(&old, new);

        assert_eq!(
            fields.get(&path),
            Some(&confirmed),
            "the user's own confirmed role (and its inferred_role) must survive untouched"
        );
        assert!(
            !change.reclassified.contains(&path),
            "a confirmed field is never reported reclassified, even though fresh \
             inference disagrees with it: {change:?}"
        );
    }

    #[test]
    fn a_field_with_no_instances_left_is_reported_removed() {
        let path: FieldPath = "gone".parse().unwrap();
        let old = BTreeMap::from([(path.clone(), spec(FieldRole::Opaque, (1, 1), None))]);
        let new = BTreeMap::new();

        let (fields, change) = reconcile_fields(&old, new);

        assert!(!fields.contains_key(&path));
        assert_eq!(change.removed, vec![path]);
    }

    #[test]
    fn a_new_field_is_reported_added() {
        let path: FieldPath = "brandNew".parse().unwrap();
        let old = BTreeMap::new();
        let added_spec = spec(FieldRole::Opaque, (1, 1), None);
        let new = BTreeMap::from([(path.clone(), added_spec.clone())]);

        let (fields, change) = reconcile_fields(&old, new);

        assert_eq!(fields.get(&path), Some(&added_spec));
        assert_eq!(change.added, vec![path]);
    }

    // -- drop_stranded_values: a reclassified field's own new role may not
    //    accept the shape its row's value was stored under. --------------

    fn standalone_project_with_field(role: FieldRole) -> rim_resolve::domain::AssignmentProject {
        let path: FieldPath = "defaultToolDef".parse().unwrap();
        let fields = BTreeMap::from([(path, spec(role, (1, 1), None))]);
        let schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields,
            target_shapes: BTreeMap::new(),
        };
        rim_resolve::domain::AssignmentProject::new(
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
            schema,
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn drop_stranded_values_strips_a_value_whose_shape_no_longer_matches_its_new_role() {
        let path: FieldPath = "defaultToolDef".parse().unwrap();
        let mut project = standalone_project_with_field(FieldRole::ItemSlot {
            def_type: "ThingDef".to_string(),
        });
        let key = RowKey::Own("test_egg".to_string());
        // Bypasses `set_row`'s own validation on purpose: this row's
        // value was legitimately stored under the field's *old* role
        // (`Scalar`) and is now stale under the new one —
        // `drop_stranded_values`'s whole job is cleaning exactly this up.
        project
            .section_mut("example.PartAssignmentDef")
            .expect("section exists")
            .rows
            .insert(
                key.clone(),
                AssignmentRow {
                    values: BTreeMap::from([(path.clone(), RowValue::Text("Stale".to_string()))]),
                    def_name: "test_egg".to_string(),
                    note: None,
                },
            );

        let dropped = drop_stranded_values(
            &mut project,
            "example.PartAssignmentDef",
            std::slice::from_ref(&path),
        );

        assert_eq!(dropped, vec![(key.clone(), path.clone())]);
        assert!(
            !project
                .section("example.PartAssignmentDef")
                .expect("section exists")
                .rows[&key]
                .values
                .contains_key(&path),
            "the stale Text value must be removed"
        );
    }

    #[test]
    fn drop_stranded_values_leaves_a_value_whose_shape_still_matches() {
        // Answers `true` only for the item slot's own referenced def
        // ("ThingDef"/"A") — never for the row's own def-name-collision
        // check ("example.PartAssignmentDef"/"test_part"), which must stay `false`
        // or `set_row` would (correctly) refuse the row as a duplicate.
        struct KnownThing;
        impl KnownDefs for KnownThing {
            fn contains(&self, def_type: &str, name: &str) -> bool {
                def_type == "ThingDef" && name == "A"
            }
        }

        let path: FieldPath = "defaultToolDef".parse().unwrap();
        let mut project = standalone_project_with_field(FieldRole::ItemSlot {
            def_type: "ThingDef".to_string(),
        });
        let key = RowKey::Own("test_part".to_string());
        project
            .set_row(
                "example.PartAssignmentDef",
                key.clone(),
                AssignmentRow {
                    values: BTreeMap::from([(
                        path.clone(),
                        RowValue::Names(vec!["A".to_string()]),
                    )]),
                    def_name: "test_part".to_string(),
                    note: None,
                },
                &KnownThing,
            )
            .expect("a Names value already matches an ItemSlot role");

        let dropped = drop_stranded_values(
            &mut project,
            "example.PartAssignmentDef",
            std::slice::from_ref(&path),
        );

        assert!(dropped.is_empty(), "{dropped:?}");
        assert!(
            project
                .section("example.PartAssignmentDef")
                .expect("section exists")
                .rows[&key]
                .values
                .contains_key(&path),
            "a value whose shape already matches its role must survive"
        );
    }

    /// End-to-end through [`UpdateAssignment::execute`]: a project stored
    /// before the `Def`-suffix exemption shipped has `defaultToolDef`
    /// as `Scalar{Text}` with a hand-typed value and no `inferred_role`.
    /// Re-running inference today (any `refs`/`excluded_refs` update)
    /// reclassifies it `ItemSlot{ThingDef}` (23 distinct raw values, only
    /// one resolving — the exact real-install shape
    /// `has_def_suffixed_leaf_tag` rescues) — the row's own stored `Text`
    /// value must be dropped and reported, not left stranded under a role
    /// that can't render it.
    #[test]
    fn a_reclassified_field_strands_its_old_shaped_value_and_reports_it() {
        let mut index = SourceIndex::default();
        let mut by_ordinal = BTreeMap::new();
        let mut mods: Vec<String> = Vec::new();
        for i in 0..23u32 {
            let mod_id = format!("mod{i}");
            let group = format!("Group{i}");
            let value = if i == 0 {
                "ThingA".to_string()
            } else {
                format!("Unresolvable{i}")
            };
            index.defs.insert(
                (
                    ModId::new(&mod_id),
                    ("example.PartAssignmentDef".to_string(), group.clone()),
                ),
                vec![DefEntry {
                    def_type: "example.PartAssignmentDef".to_string(),
                    def_name: group.clone(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(i),
                }],
            );
            index.owners_by_def.insert(
                ("example.PartAssignmentDef".to_string(), group.clone()),
                vec![ModId::new(&mod_id)],
            );
            by_ordinal.insert(i,
                format!("<example.PartAssignmentDef><defName>{group}</defName><defaultToolDef>{value}</defaultToolDef></example.PartAssignmentDef>"
                ));
            mods.push(mod_id);
        }
        index
            .defs_by_name
            .entry("ThingA".to_string())
            .or_default()
            .push(("ThingDef".to_string(), ModId::new("framework")));

        let report = {
            let mut builder = ReportBuilder::new();
            for m in &mods {
                builder = builder.mod_(m);
            }
            builder.build()
        };
        let mod_refs: Vec<&str> = mods.iter().map(String::as_str).collect();
        let mut session = session_with_sources_and_mods(index, report, &mod_refs);

        let egg_path: FieldPath = "defaultToolDef".parse().unwrap();
        let schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::from([(
                egg_path.clone(),
                spec(
                    FieldRole::Scalar {
                        kind: ScalarKind::Text,
                        default: None,
                    },
                    (23, 23),
                    None,
                ),
            )]),
            target_shapes: BTreeMap::new(),
        };
        let mut project = rim_resolve::domain::AssignmentProject::new(
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
            schema,
            jiff::Timestamp::UNIX_EPOCH,
        );
        let key = RowKey::Own("test_egg".to_string());
        project
            .set_row(
                "example.PartAssignmentDef",
                key.clone(),
                AssignmentRow {
                    values: BTreeMap::from([(
                        egg_path.clone(),
                        RowValue::Text("Unresolvable5".to_string()),
                    )]),
                    def_name: "test_egg".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("valid row under the old Scalar role");
        let id = project.id().clone();
        session.upsert_assignment(project);

        let use_case = UpdateAssignment::new(
            InMemoryAssignmentProjectStore::new(),
            FakeReader { by_ordinal },
        );
        let outcome = use_case
            .execute(
                &mut session,
                &id,
                UpdateAssignmentInput {
                    refs: Some(BTreeSet::new()),
                    ..UpdateAssignmentInput::default()
                },
            )
            .expect("update must succeed");

        let change = outcome
            .schema_changes
            .get("example.PartAssignmentDef")
            .expect("refs were part of the update, so every section gets an entry");
        assert!(change.reclassified.contains(&egg_path), "{change:?}");
        assert_eq!(
            outcome.stranded_values,
            vec![(
                "example.PartAssignmentDef".to_string(),
                key.clone(),
                egg_path.clone()
            )]
        );
        let after = session.assignment(&id).expect("still loaded");
        assert!(
            !after
                .section("example.PartAssignmentDef")
                .expect("the fixture's own section")
                .rows[&key]
                .values
                .contains_key(&egg_path),
            "the stranded Text value must be dropped, not left under an ItemSlot role \
             that can't render it"
        );
    }
}
