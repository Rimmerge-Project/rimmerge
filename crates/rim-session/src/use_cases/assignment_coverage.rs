//! [`AssignmentCoverage`]: the patch maker's coverage work queue, built
//! from the active install over the pure [`coverage`]: every candidate
//! target of the
//! project's own key type(s) owned by T and matching the learned
//! [`TargetShape`], plus every existing assignment instance (any active
//! mod, in or out of R/T) that already references one.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{GeneratedKind, LoadOrder, ModId, Selector};
use rim_merge::tree::Content;
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, Coverage, DefKey, ExistingInstance, FieldPath, FieldRole,
    TargetRef, TargetShape, coverage as compute_coverage,
};

use super::assignment_instances::AssignmentInstancesError;
use super::def_sources::{self, DefSourceLookupError};
use crate::Session;
use crate::ports::DefSourceReader;

/// Everything that can go wrong building one assignment project's
/// coverage work queue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentCoverageError {
    /// No assignment project with the given id is loaded.
    #[error(transparent)]
    Unknown(#[from] crate::UnknownAssignment),
    /// Reading an existing instance's, or a candidate target's, XML failed.
    #[error(transparent)]
    Source(#[from] AssignmentInstancesError),
    /// Resolving a candidate target's inherited tree failed.
    #[error("resolving a candidate target's inherited tree: {0}")]
    Inherit(String),
}

impl From<DefSourceLookupError> for AssignmentCoverageError {
    fn from(error: DefSourceLookupError) -> Self {
        Self::Source(error.into())
    }
}

/// Every [`FieldRole::TargetKey`] field in `def_type`'s own section
/// schema, with its resolved key type. Empty when `project` has no
/// section for `def_type` (the caller's own `Coverage { applicable: false,
/// .. }` short-circuit already handles that case before this is ever
/// called with a bogus `def_type`, but this stays total rather than
/// panicking on a caller mistake).
fn target_key_fields(project: &AssignmentProject, def_type: &str) -> Vec<(FieldPath, String)> {
    let Some(section) = project.section(def_type) else {
        return Vec::new();
    };
    section
        .schema
        .fields
        .iter()
        .filter_map(|(path, spec)| match &spec.role {
            FieldRole::TargetKey { def_type } => Some((path.clone(), def_type.clone())),
            _ => None,
        })
        .collect()
}

/// `def_name`'s own winning owner (under `order`, [`Selector::DefName`]'s
/// last-loaded-wins rule) plus its fully inherited tree's top-level child
/// tags — the same "raw + template chain + inherit::resolve" read
/// `create_assignment.rs`'s own target-shape *inference* needs, kept as
/// its own copy here for candidate-shape *matching* (a different
/// question — "does this one candidate satisfy an already-learned shape"
/// — with its own error type) rather than shared across the two use
/// cases, per this codebase's "tolerate minor duplication" convention.
fn owner_and_resolved_children<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    def_type: &str,
    def_name: &str,
) -> Result<(ModId, BTreeSet<String>), AssignmentCoverageError> {
    let (owner, raw) = def_sources::def_owner_and_raw(
        reader,
        session,
        order,
        def_type,
        def_name,
        Selector::DefName,
    )?;
    let templates = def_sources::template_set(
        reader,
        session,
        order,
        def_type,
        &owner,
        raw.parent_name.as_deref(),
    )?;
    let resolved = rim_merge::inherit::resolve(&raw, &templates)
        .map_err(|error| AssignmentCoverageError::Inherit(error.to_string()))?;
    let children = match &resolved.root.content {
        Content::Children(children) => children.iter().map(|child| child.tag.clone()).collect(),
        _ => BTreeSet::new(),
    };
    Ok((owner, children))
}

/// Builds coverage's `candidates`: every def of a target-key
/// field's own resolved type, owned by a member of `project.targets()`
/// (by [`ModId::base`]), whose resolved top-level children satisfy that
/// field's own learned [`TargetShape`] — an unfiltered shape (no learned
/// requirement, or none recorded at all for that field) admits every def
/// of the type.
fn build_candidates<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    project: &AssignmentProject,
    def_type: &str,
) -> Result<BTreeMap<TargetRef, ModId>, AssignmentCoverageError> {
    let targets: BTreeSet<ModId> = project.targets().iter().map(ModId::base).collect();
    let mut candidates = BTreeMap::new();

    for (key_field, key_type) in target_key_fields(project, def_type) {
        let shape = project
            .section(def_type)
            .unwrap_or_else(|| unreachable!("target_key_fields above already confirmed a section"))
            .schema
            .target_shapes
            .get(&key_field)
            .cloned()
            .unwrap_or_else(|| TargetShape {
                def_type: key_type.clone(),
                required_children: BTreeSet::new(),
            });
        let def_names: BTreeSet<String> = session
            .sources()
            .owners_by_def
            .iter()
            .filter(|((def_type, _), owners)| {
                *def_type == key_type && owners.iter().any(|owner| targets.contains(&owner.base()))
            })
            .map(|((_, def_name), _)| def_name.clone())
            .collect();

        for def_name in def_names {
            let (owner, children) =
                owner_and_resolved_children(reader, session, order, &key_type, &def_name)?;
            if !targets.contains(&owner.base()) {
                // Some other, non-target mod's copy actually wins under
                // the selected order — not a real candidate.
                continue;
            }
            if !shape.matches(&children) {
                continue;
            }
            candidates.insert(
                TargetRef {
                    key_field: key_field.clone(),
                    def: DefKey {
                        def_type: key_type.clone(),
                        def_name,
                    },
                },
                owner,
            );
        }
    }

    Ok(candidates)
}

/// Whether `owner` is this project's own previously exported instance
/// (a hand-renamed export included): matched by package id first
/// (`owner.base()` against the
/// project's own published package id), then by the owner's own
/// `rimmerge.json` marker naming this project's id (a hand-renamed
/// export's own package id no longer matches, but its marker still does)
/// — either match excludes it from `existing`, so it is never ranked
/// twice under two different identities once active.
fn is_own_export(session: &Session, owner: &ModId, project: &AssignmentProject) -> bool {
    if owner.base() == project.identity().package_id().base() {
        return true;
    }
    session
        .report()
        .mods
        .iter()
        .find(|m| m.id.base() == owner.base())
        .and_then(|m| m.generated.as_ref())
        .is_some_and(|marker| {
            marker.kind == GeneratedKind::Assignment
                && marker.patch_id.as_deref() == Some(project.id().as_str())
        })
}

/// Builds coverage's `existing`: every active instance of
/// `project`'s own assignment def type, from **any** active mod (in or
/// out of R/T — coverage is about what the whole install already covers),
/// read directly rather than through [`super::AssignmentInstances`]'s own
/// cache (which deliberately drops each instance's own `defName` —
/// exactly the fact [`ExistingInstance::instance_def_name`] needs), one
/// entry per [`FieldRole::TargetKey`] value it names. Filters out this
/// project's own previous export ([`is_own_export`]) so it is never
/// double-counted once active — once under its own `defName` here, and
/// once as [`rim_resolve::domain::Winner::ThisProject`].
fn build_existing<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    project: &AssignmentProject,
    def_type: &str,
) -> Result<Vec<ExistingInstance>, AssignmentCoverageError> {
    let key_fields = target_key_fields(project, def_type);

    let def_names: BTreeSet<String> = session
        .sources()
        .owners_by_def
        .iter()
        .filter(|((candidate_type, _), _)| candidate_type.as_str() == def_type)
        .map(|((_, def_name), _)| def_name.clone())
        .collect();

    let mut existing = Vec::new();
    for def_name in &def_names {
        let (owner, tree) = def_sources::def_owner_and_raw(
            reader,
            session,
            order,
            def_type,
            def_name,
            Selector::DefName,
        )?;
        if is_own_export(session, &owner, project) {
            continue;
        }
        let instance = rim_merge::assign::read_instance(&tree);
        for (key_field, key_type) in &key_fields {
            let Some(occurrence) = instance.get(key_field) else {
                continue;
            };
            for target_name in &occurrence.values {
                existing.push(ExistingInstance {
                    owner: owner.clone(),
                    instance_def_name: def_name.clone(),
                    key_field: key_field.clone(),
                    target: DefKey {
                        def_type: key_type.clone(),
                        def_name: target_name.clone(),
                    },
                });
            }
        }
    }

    Ok(existing)
}

/// Builds (and caches) one assignment project's coverage work queue.
pub struct AssignmentCoverage<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> AssignmentCoverage<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Builds (or returns the cached) coverage work queue for `def_type`'s
    /// own section of `id`'s project — coverage applies per target-keyed
    /// section, not per project.
    ///
    /// # Errors
    ///
    /// See [`AssignmentCoverageError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        def_type: &str,
    ) -> Result<Coverage, AssignmentCoverageError> {
        let source = session.selected();
        if let Some(cached) = session.cached_assignment_coverage(source, id, def_type) {
            return Ok(cached.clone());
        }

        let project = session
            .assignment(id)
            .cloned()
            .ok_or_else(|| crate::UnknownAssignment(id.clone()))?;
        // A missing section, or a free-standing ("new def") one, has no
        // `TargetKey` field at all — coverage's whole premise (which
        // candidate targets already have a matching instance) has no
        // meaning without one, so this is reported as "not applicable"
        // rather than a vacuously empty (and therefore misleadingly
        // "fully covered") queue. `rim_resolve::domain::coverage` already
        // returns this same result for either case; short-circuiting here
        // too just skips the session-layer IO (`build_candidates`/
        // `build_existing`) that result would otherwise never use.
        let is_target_keyed = project
            .section(def_type)
            .is_some_and(|section| !section.is_standalone());
        if !is_target_keyed {
            let result = Coverage {
                applicable: false,
                rows: Vec::new(),
            };
            session.cache_assignment_coverage(
                source,
                id.clone(),
                def_type.to_string(),
                result.clone(),
            );
            return Ok(result);
        }
        let order = session.orders().get(source).clone();

        let candidates = build_candidates(&self.reader, session, &order, &project, def_type)?;
        let existing = build_existing(&self.reader, session, &order, &project, def_type)?;
        // Which def type has a verified precedence rule is data, loaded
        // once at `LoadProject` time — never a table in this workspace's source.
        let rule = session.mod_knowledge().precedence_for(def_type);

        let result = compute_coverage(&project, def_type, &candidates, &existing, &order, &rule);
        session.cache_assignment_coverage(source, id.clone(), def_type.to_string(), result.clone());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{DefSourceError, ElementExpectation};
    use crate::test_support::{assignment_fixture_with_sources, report_fixture};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, GeneratedMarker, XmlLocator};
    use rim_resolve::domain::{Cardinality, ExistingMatch, FieldSpec, RowIntent, Winner};
    use std::collections::BTreeMap as Map;
    use std::path::Path;
    use std::sync::Arc;

    struct FakeReader {
        by_ordinal: Map<u32, String>,
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

    fn schema() -> rim_resolve::domain::AssignmentSchema {
        let mut fields = Map::new();
        fields.insert(
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
        rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::from([ModId::new("fixture.framework")]),
            fields,
            target_shapes: Map::new(),
        }
    }

    /// A target mod (`fixture.target`) owning two `ThingDef`s (`Elf`,
    /// `Dwarf`), plus an `existing` mod already shipping one
    /// `example.PartAssignmentDef` instance (`Group_Dwarf`) referencing `Dwarf`
    /// — enough to exercise both a covered and an uncovered candidate.
    fn fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        let mut ordinal = 0u32;
        let mut by_ordinal = Map::new();

        for name in ["Elf", "Dwarf"] {
            index.defs.insert(
                (
                    ModId::new("fixture.target"),
                    ("ThingDef".to_string(), name.to_string()),
                ),
                vec![DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: name.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(ordinal),
                }],
            );
            index.owners_by_def.insert(
                ("ThingDef".to_string(), name.to_string()),
                vec![ModId::new("fixture.target")],
            );
            by_ordinal.insert(
                ordinal,
                format!("<ThingDef><defName>{name}</defName></ThingDef>"),
            );
            ordinal += 1;
        }

        index.defs.insert(
            (
                ModId::new("fixture.existing"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "Group_Dwarf".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "Group_Dwarf".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "Group_Dwarf".to_string(),
            ),
            vec![ModId::new("fixture.existing")],
        );
        by_ordinal.insert(ordinal,
            "<example.PartAssignmentDef><defName>Group_Dwarf</defName><speciesNames><li>Dwarf</li></speciesNames></example.PartAssignmentDef>".to_string());

        (index, FakeReader { by_ordinal })
    }

    fn report() -> rim_analyzer::domain::Report {
        report_fixture(&["fixture.target", "fixture.existing"])
    }

    #[test]
    fn dwarf_is_covered_by_the_other_mods_instance_elf_is_not() {
        let (sources, reader) = fixture();
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report(),
            &["fixture.target", "fixture.existing"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        let use_case = AssignmentCoverage::new(reader);

        let coverage = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("coverage must succeed");

        let dwarf = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "Dwarf")
            .expect("Dwarf must be a candidate");
        assert_eq!(dwarf.intent, RowIntent::Override);
        assert_eq!(dwarf.matches.len(), 1);
        assert_eq!(dwarf.matches[0].owner, ModId::new("fixture.existing"));
        assert_eq!(dwarf.matches[0].instance_def_name, "Group_Dwarf");
        // No built-in rule for `example.PartAssignmentDef`: `Unverified` names
        // no winner.
        assert!(dwarf.winner.is_none());

        let elf = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "Elf")
            .expect("Elf must be a candidate");
        assert_eq!(elf.intent, RowIntent::Cover);
        assert!(elf.matches.is_empty());
    }

    #[test]
    fn a_row_this_project_already_has_is_reported_uncovered_but_has_row_true() {
        let (sources, reader) = fixture();
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report(),
            &["fixture.target", "fixture.existing"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        let mut project = session
            .assignment(&id)
            .cloned()
            .expect("project must be loaded");
        project
            .set_row(
                "example.PartAssignmentDef",
                rim_resolve::domain::RowKey::Target(TargetRef {
                    key_field: "speciesNames".parse().unwrap(),
                    def: DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Elf".to_string(),
                    },
                }),
                rim_resolve::domain::AssignmentRow {
                    values: Map::new(),
                    def_name: "mypatch_partassign_Elf".to_string(),
                    note: None,
                },
                &crate::assignment_refs::SessionKnownDefs::new(
                    &session,
                    project.identity().package_id().clone(),
                    // `session`'s own still-unmutated copy — a separate
                    // binding from `project`, so borrowing it here doesn't
                    // conflict with mutating `project` in this same call.
                    session.assignment(&id).expect("still loaded"),
                ),
            )
            .expect("a valid TargetKey row");
        session.upsert_assignment(project);
        let use_case = AssignmentCoverage::new(reader);

        let coverage = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("coverage must succeed");

        let elf = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "Elf")
            .expect("Elf must be a candidate");
        assert!(elf.has_row);
        assert_eq!(
            elf.intent,
            RowIntent::Cover,
            "no *existing* instance references Elf, regardless of this project's own row"
        );
    }

    #[test]
    fn this_projects_own_previous_export_is_excluded_from_existing() {
        let (mut sources, reader) = fixture();
        // Simulate this project's own previous export being active: a
        // third mod, sharing this fixture's fixed `"test.assignment"`
        // package id, already ships an instance naming Elf.
        sources.defs.insert(
            (
                ModId::new("test.assignment"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "own_Elf".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "own_Elf".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(1000),
            }],
        );
        sources.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "own_Elf".to_string(),
            ),
            vec![ModId::new("test.assignment")],
        );
        let reader = FakeReader {
            by_ordinal: {
                let mut map = reader.by_ordinal;
                map.insert(1000,
                    "<example.PartAssignmentDef><defName>own_Elf</defName><speciesNames><li>Elf</li></speciesNames></example.PartAssignmentDef>".to_string());
                map
            },
        };
        let report = report_fixture(&["fixture.target", "fixture.existing", "test.assignment"]);

        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report,
            &["fixture.target", "fixture.existing", "test.assignment"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        let use_case = AssignmentCoverage::new(reader);

        let coverage = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("coverage must succeed");

        let elf = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "Elf")
            .expect("Elf must be a candidate");
        assert!(
            elf.matches.is_empty(),
            "this project's own prior export must never appear as an ExistingMatch: {elf:?}"
        );
    }

    /// A standalone ("new def") project's schema has no `TargetKey` field
    /// — coverage is reported "not applicable", never a vacuous empty
    /// queue that would misleadingly read as "fully covered".
    #[test]
    fn a_standalone_project_reports_coverage_not_applicable() {
        let (sources, reader) = fixture();
        let standalone_schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::from([ModId::new("fixture.framework")]),
            fields: Map::new(),
            target_shapes: Map::new(),
        };
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report(),
            &["fixture.target", "fixture.existing"],
            &["fixture.framework"],
            &[],
            standalone_schema,
        );
        let use_case = AssignmentCoverage::new(reader);

        let coverage = use_case
            .execute(&mut session, &id, "example.PartDef")
            .expect("coverage must succeed even though it isn't applicable");

        assert!(!coverage.applicable);
        assert!(coverage.rows.is_empty());
    }

    /// Coverage is per
    /// target-keyed section — a project with both a free-standing and a
    /// target-keyed section reports "not applicable" for the former and a
    /// normal work queue for the latter, from the same project.
    #[test]
    fn coverage_is_not_applicable_for_a_free_standing_section_and_normal_for_a_target_keyed_one() {
        let (sources, reader) = fixture();
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report(),
            &["fixture.target", "fixture.existing"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        let mut project = session.assignment(&id).cloned().expect("loaded");
        project
            .add_section(rim_resolve::domain::AssignmentSchema {
                def_type: "example.PartDef".to_string(),
                refs: BTreeSet::from([ModId::new("fixture.framework")]),
                fields: Map::new(),
                target_shapes: Map::new(),
            })
            .expect("a fresh def type must add cleanly");
        session.upsert_assignment(project);
        let use_case = AssignmentCoverage::new(reader);

        let standalone_coverage = use_case
            .execute(&mut session, &id, "example.PartDef")
            .expect("coverage must succeed even though it isn't applicable");
        assert!(!standalone_coverage.applicable);
        assert!(standalone_coverage.rows.is_empty());

        let target_keyed_coverage = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("coverage must succeed");
        assert!(target_keyed_coverage.applicable);
        assert!(
            target_keyed_coverage
                .rows
                .iter()
                .any(|row| row.target.def.def_name == "Elf"),
            "{target_keyed_coverage:?}"
        );
    }

    #[test]
    fn a_second_call_reuses_the_cache() {
        let (sources, reader) = fixture();
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report(),
            &["fixture.target", "fixture.existing"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        let use_case = AssignmentCoverage::new(reader);
        let first = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("first call must succeed");

        // A reader with no seeded content: if this returns `Ok`, it must
        // have come from the cache, not a second read.
        let use_case_that_would_fail = AssignmentCoverage::new(FakeReader {
            by_ordinal: Map::new(),
        });
        let second = use_case_that_would_fail
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("a cache hit must succeed even though this reader would fail on a real read");

        assert_eq!(first, second);
    }

    /// `GeneratedMarker`-only exclusion: `is_own_export` must still
    /// recognize a hand-renamed export's own instance by its marker's
    /// `assignmentId` even when its package id no longer matches the
    /// project's own. [`assignment_fixture_with_sources`]'s own project
    /// always carries the same fixed, deterministically derived id (`"test.assignment"`'s
    /// own package id, `"profile"`, `jiff::Timestamp::UNIX_EPOCH`), so it
    /// can be computed up front and baked into the report's own marker
    /// from the start, with no need to build a project first just to learn
    /// it.
    #[test]
    fn is_own_export_matches_by_marker_when_the_package_id_was_renamed() {
        let expected_id = rim_resolve::domain::AssignmentId::derive(
            "profile",
            &ModId::new("test.assignment"),
            jiff::Timestamp::UNIX_EPOCH,
        );
        let (sources, _reader) = fixture();
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("fixture.target")
            .mod_("fixture.existing")
            .mod_with("renamed.export", |m| {
                m.generated = Some(GeneratedMarker {
                    kind: GeneratedKind::Assignment,
                    patch_id: Some(expected_id.as_str().to_string()),
                    scope: Some(BTreeSet::new()),
                });
            })
            .build();
        let (session, id) = assignment_fixture_with_sources(
            sources,
            report,
            &["fixture.target", "fixture.existing", "renamed.export"],
            &["fixture.framework"],
            &["fixture.target"],
            schema(),
        );
        assert_eq!(id, expected_id, "sanity check on the fixture's own id");
        let project = session.assignment(&id).expect("loaded").clone();

        assert!(is_own_export(
            &session,
            &ModId::new("renamed.export"),
            &project
        ));
    }

    // -- a *loaded* precedence rule, exercised above
    // `rim-resolve` (the string-key `def_type` -> `ModKnowledge` ->
    // `PrecedenceRule::winner` wiring `coverage()`'s own tests can't
    // reach, since they call `winner` directly).
    //
    // The rule here is injected through `Session::set_mod_knowledge`, the
    // same way `LoadProject` injects the real one: which def type has a
    // verified rule is data, so a test that hardcoded one would be pinning
    // a table this workspace does not have. The invented def
    // type/framework are neutral vocabulary; the *shape* (two `TargetKey`
    // fields with a priority between them, a framework whose own instances
    // lose) is what has teeth. -----------------------------------------

    fn framework_schema() -> rim_resolve::domain::AssignmentSchema {
        let mut fields = Map::new();
        fields.insert(
            "kindNames".parse().unwrap(),
            FieldSpec {
                role: FieldRole::TargetKey {
                    def_type: "PawnKindDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        fields.insert(
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
        rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::from([ModId::new("example.framework")]),
            fields,
            target_shapes: Map::new(),
        }
    }

    /// `fixture.target` owns both `ElfKind` (`PawnKindDef`) and `SomeRace`
    /// (`ThingDef`); `example.framework` (the built-in rule's own `framework`)
    /// ships one existing instance naming both, via `kindNames` and
    /// `speciesNames` respectively; `example.addon` ships a second, naming only
    /// `SomeRace` via `speciesNames`.
    fn framework_fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        let mut ordinal = 0u32;
        let mut by_ordinal = Map::new();

        for (def_type, name) in [("PawnKindDef", "ElfKind"), ("ThingDef", "SomeRace")] {
            index.defs.insert(
                (
                    ModId::new("fixture.target"),
                    (def_type.to_string(), name.to_string()),
                ),
                vec![DefEntry {
                    def_type: def_type.to_string(),
                    def_name: name.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(ordinal),
                }],
            );
            index.owners_by_def.insert(
                (def_type.to_string(), name.to_string()),
                vec![ModId::new("fixture.target")],
            );
            by_ordinal.insert(
                ordinal,
                format!("<{def_type}><defName>{name}</defName></{def_type}>"),
            );
            ordinal += 1;
        }

        index.defs.insert(
            (
                ModId::new("example.framework"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "Group_Framework".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "Group_Framework".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "Group_Framework".to_string(),
            ),
            vec![ModId::new("example.framework")],
        );
        by_ordinal.insert(
            ordinal,
            "<example.PartAssignmentDef><defName>Group_Framework</defName>\
             <kindNames><li>ElfKind</li></kindNames>\
             <speciesNames><li>SomeRace</li></speciesNames></example.PartAssignmentDef>"
                .to_string(),
        );
        ordinal += 1;

        index.defs.insert(
            (
                ModId::new("example.addon"),
                (
                    "example.PartAssignmentDef".to_string(),
                    "Group_Addon".to_string(),
                ),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: "Group_Addon".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            (
                "example.PartAssignmentDef".to_string(),
                "Group_Addon".to_string(),
            ),
            vec![ModId::new("example.addon")],
        );
        by_ordinal.insert(
            ordinal,
            "<example.PartAssignmentDef><defName>Group_Addon</defName>\
             <speciesNames><li>SomeRace</li></speciesNames></example.PartAssignmentDef>"
                .to_string(),
        );

        (index, FakeReader { by_ordinal })
    }

    #[test]
    fn a_loaded_precedence_rule_names_this_project_for_a_kind_row_and_an_addon_for_an_uncontested_species()
     {
        let (sources, reader) = framework_fixture();
        let report = report_fixture(&["fixture.target", "example.framework", "example.addon"]);
        let (mut session, id) = assignment_fixture_with_sources(
            sources,
            report,
            &["fixture.target", "example.framework", "example.addon"],
            &["example.framework"],
            &["fixture.target"],
            framework_schema(),
        );
        let mut project = session
            .assignment(&id)
            .cloned()
            .expect("project must be loaded");
        let known = crate::assignment_refs::SessionKnownDefs::new(
            &session,
            project.identity().package_id().clone(),
            session.assignment(&id).expect("still loaded"),
        );
        project
            .set_row(
                "example.PartAssignmentDef",
                rim_resolve::domain::RowKey::Target(TargetRef {
                    key_field: "kindNames".parse().unwrap(),
                    def: DefKey {
                        def_type: "PawnKindDef".to_string(),
                        def_name: "ElfKind".to_string(),
                    },
                }),
                rim_resolve::domain::AssignmentRow {
                    values: Map::new(),
                    def_name: "test_assignment_ElfKind".to_string(),
                    note: None,
                },
                &known,
            )
            .expect("a valid pawn-kind row");
        session.upsert_assignment(project);
        session.set_mod_knowledge(crate::ModKnowledge::new(
            Map::from([(
                "example.PartAssignmentDef".to_string(),
                rim_resolve::domain::PrecedenceRule::PreferOutsideFramework {
                    framework: ModId::new("example.framework"),
                    key_priority: vec![
                        rim_resolve::domain::top_level_field("kindNames"),
                        rim_resolve::domain::top_level_field("speciesNames"),
                    ],
                },
            )]),
            rim_merge::patch_behaviours::PatchOperationBehaviours::default(),
            Vec::new(),
            crate::ports::LogShapes::default(),
        ));
        let use_case = AssignmentCoverage::new(reader);

        let coverage = use_case
            .execute(&mut session, &id, "example.PartAssignmentDef")
            .expect("coverage must succeed");

        // This project's own row wins its target: it's owned outside the
        // framework (`example.framework`), which `PreferOutsideFramework`
        // always prefers over the framework's own competing match.
        let elf_kind = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "ElfKind")
            .expect("ElfKind must be a candidate");
        assert!(elf_kind.has_row);
        assert_eq!(
            elf_kind.winner,
            Some(Winner::ThisProject),
            "owned outside the framework must beat example.framework's own existing match: {elf_kind:?}"
        );

        // No row of this project's own touches `SomeRace`; between the two
        // *existing* matches, `example.addon` (outside the framework) beats
        // `example.framework`'s own (the framework itself).
        let some_race = coverage
            .rows
            .iter()
            .find(|row| row.target.def.def_name == "SomeRace")
            .expect("SomeRace must be a candidate");
        assert!(!some_race.has_row);
        assert_eq!(
            some_race.winner,
            Some(Winner::Existing(ExistingMatch {
                owner: ModId::new("example.addon"),
                instance_def_name: "Group_Addon".to_string(),
                key_field: "speciesNames".parse().unwrap(),
            })),
            "example.addon owns outside the framework and must beat example.framework's own match: {some_race:?}"
        );
    }
}
