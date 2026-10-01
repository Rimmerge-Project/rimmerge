//! Patch-project and assignment-project fixture builders.

use std::collections::BTreeMap;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{ModId, Report};
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, AssignmentSchema, Cardinality, DecisionSet, FieldRole,
    FieldSpec, PatchId, PatchModIdentity, PatchProject, PatchScope,
};

use crate::ports::{ModsConfigFile, StoredRules};
use crate::test_support::{session_fixture, session_with_sources, session_with_sources_and_mods};
use crate::{ProjectPaths, Session};

/// Builds a [`Session`] over `active_mods` (via [`session_fixture`]) with
/// one compat patch project already loaded and visible, scoped to
/// `scope_members`. The project's own package id/display name are fixed
/// (`"test.patch"`/`"Test Patch"` — no test needs more than one project at
/// a time to exercise) and its report is a plain [`report_fixture`], with
/// no real findings behind any of the keys a test might decide on: fine
/// for every use case whose own validation is purely key-vs-scope
/// (`CreatePatch`/`UpdatePatch`/`DeletePatch`/`DecidePatch`/
/// `RevertPatchDecision`/`PrunePatchDecisions`), but a test that needs a
/// *real* finding (`DecidePatchMerge`, or a scoped-ledger paging test)
/// should build its own session via [`patch_fixture_with_sources`]
/// instead.
#[must_use]
pub fn patch_fixture(active_mods: &[&str], scope_members: &[&str]) -> (Session, PatchId) {
    let mut session = session_fixture(active_mods);
    let scope = PatchScope::new(scope_members.iter().map(|id| ModId::new(*id)))
        .unwrap_or_else(|error| panic!("patch_fixture needs >=2 distinct scope members: {error}"));
    let (project, id) = new_test_patch_project(scope);
    session.upsert_patch(project);
    (session, id)
}

/// [`patch_fixture`], but built over a real [`SourceIndex`]/[`Report`]
/// (e.g. [`bionic_heart_fixture`]) — for a test that needs
/// [`crate::use_cases::PlanMerge`]/[`crate::use_cases::DecideMerge`]-style
/// real merge data, or a scoped ledger with genuine findings behind it.
#[must_use]
pub fn patch_fixture_with_sources(
    sources: SourceIndex,
    report: Report,
    scope: PatchScope,
) -> (Session, PatchId) {
    let mut session = session_with_sources(sources, report);
    let (project, id) = new_test_patch_project(scope);
    session.upsert_patch(project);
    (session, id)
}

/// [`patch_fixture_with_sources`], but for an arbitrary active-mod list (in
/// order) — [`session_with_sources_and_mods`]'s own patch-fixture sibling,
/// needed wherever a fixture's owners aren't exactly
/// `ludeon.rimworld`/`example.bionicsfork` (e.g.
/// [`whole_def_patch_collision_fixture`]'s `core.mod`/`a.mod`).
#[must_use]
pub fn patch_fixture_with_sources_and_mods(
    sources: SourceIndex,
    report: Report,
    scope: PatchScope,
    active_mods: &[&str],
) -> (Session, PatchId) {
    let mut session = session_with_sources_and_mods(sources, report, active_mods);
    let (project, id) = new_test_patch_project(scope);
    session.upsert_patch(project);
    (session, id)
}

/// [`patch_fixture_with_sources`], but with a caller-supplied
/// [`ProjectPaths`] rather than every other fixture's fixed, relative
/// `game_dir: "game"` — for a test that needs a *realistic*, absolute
/// `game_dir` to exercise path comparisons against it (e.g.
/// `export_patch`'s own inside-`Mods`/absolute-path checks), where the
/// usual relative fixture path can never plausibly be absolute.
#[must_use]
pub fn patch_fixture_with_paths(
    sources: SourceIndex,
    report: Report,
    scope: PatchScope,
    paths: ProjectPaths,
) -> (Session, PatchId) {
    let mut session = Session::new(
        paths,
        report,
        Vec::new(),
        sources,
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: vec![
                ModId::new("ludeon.rimworld"),
                ModId::new("example.bionicsfork"),
            ],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    let (project, id) = new_test_patch_project(scope);
    session.upsert_patch(project);
    (session, id)
}

/// A fixed-identity, freshly created (no decisions, no export) patch
/// project over `scope` — the one construction every `patch_fixture*`
/// helper shares.
fn new_test_patch_project(scope: PatchScope) -> (PatchProject, PatchId) {
    let identity = PatchModIdentity::new("test.patch", "Test Patch")
        .unwrap_or_else(|error| unreachable!("a fixed, valid identity: {error}"));
    let id = PatchId::derive(
        "profile",
        identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let project = PatchProject::new(
        id.clone(),
        "Test Patch Project".to_string(),
        identity,
        scope,
        jiff::Timestamp::UNIX_EPOCH,
    );
    (project, id)
}

/// Builds a [`Session`] over `active_mods` (via [`session_fixture`]) with
/// one assignment (patch maker) project already loaded and visible: a
/// fixed identity (`"test.assignment"`/`"Test Assignment"` — no test
/// needs more than one project at a time to exercise), `refs`/`targets` as
/// given, and a minimal one-field schema (`speciesNames`, a
/// [`FieldRole::TargetKey`] typed `ThingDef`) — enough for any use case
/// whose own validation is purely row-vs-schema
/// (`SetAssignmentRow`/`ClearAssignmentRow`/`DeleteAssignment`/
/// `UpdateAssignment`), mirroring [`patch_fixture`]'s own scope.
#[must_use]
pub fn assignment_fixture(
    active_mods: &[&str],
    refs: &[&str],
    targets: &[&str],
) -> (Session, AssignmentId) {
    let mut session = session_fixture(active_mods);
    let (project, id) = new_test_assignment_project(refs, targets);
    session.upsert_assignment(project);
    (session, id)
}

/// A fixed-identity, freshly created (no rows, no export) assignment
/// project over `refs`/`targets` with a minimal one-field schema
/// (`speciesNames`, a [`FieldRole::TargetKey`] typed `ThingDef`) — the one
/// construction [`assignment_fixture`] shares.
fn new_test_assignment_project(
    refs: &[&str],
    targets: &[&str],
) -> (AssignmentProject, AssignmentId) {
    let mut fields = BTreeMap::new();
    fields.insert(
        "speciesNames"
            .parse::<rim_resolve::domain::FieldPath>()
            .unwrap_or_else(|error| unreachable!("a fixed, valid field path: {error}")),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let schema = AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: refs.iter().map(|id| ModId::new(*id)).collect(),
        fields,
        target_shapes: BTreeMap::new(),
    };
    new_test_assignment_project_with_schema(refs, targets, schema)
}

/// [`new_test_assignment_project`], but with a caller-supplied `schema`
/// instead of the fixed minimal one — for a fixture whose schema must
/// actually resolve against real owner/tree data (`AssignmentCoverage`/
/// `ExportAssignment` tests), unlike the row-vs-schema validation tests
/// `new_test_assignment_project`'s own callers exercise.
fn new_test_assignment_project_with_schema(
    refs: &[&str],
    targets: &[&str],
    schema: AssignmentSchema,
) -> (AssignmentProject, AssignmentId) {
    let identity = PatchModIdentity::new("test.assignment", "Test Assignment")
        .unwrap_or_else(|error| unreachable!("a fixed, valid identity: {error}"));
    let id = AssignmentId::derive(
        "profile",
        identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let project = AssignmentProject::new(
        id.clone(),
        "Test Assignment Project".to_string(),
        identity,
        refs.iter().map(|id| ModId::new(*id)).collect(),
        targets.iter().map(|id| ModId::new(*id)).collect(),
        schema,
        jiff::Timestamp::UNIX_EPOCH,
    );
    (project, id)
}

/// [`assignment_fixture_with_sources`], but with a caller-supplied
/// [`ProjectPaths`] rather than every other fixture's fixed, relative
/// `game_dir: "game"` — for a test that needs a *realistic*, absolute
/// `game_dir` to exercise path comparisons against it (mirrors
/// [`patch_fixture_with_paths`]'s own reasoning, e.g.
/// `export_assignment`'s inside-`Mods`/absolute-path checks).
#[must_use]
pub fn assignment_fixture_with_paths(
    sources: SourceIndex,
    report: Report,
    active_mods: &[&str],
    refs: &[&str],
    targets: &[&str],
    schema: AssignmentSchema,
    paths: ProjectPaths,
) -> (Session, AssignmentId) {
    let mut session = Session::new(
        paths,
        report,
        Vec::new(),
        sources,
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: active_mods.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    let (project, id) = new_test_assignment_project_with_schema(refs, targets, schema);
    session.upsert_assignment(project);
    (session, id)
}

/// [`patch_fixture_with_sources`]'s assignment twin: a session over a real
/// `SourceIndex`/`Report` with an arbitrary active-mod list (unlike
/// `assignment_fixture`'s empty index and fixed `active_mods`-only
/// session), with one assignment project already loaded, built from
/// caller-supplied `refs`/`targets`/`schema`
/// ([`new_test_assignment_project_with_schema`]) — for
/// `AssignmentCoverage`/`ExportAssignment` tests, which need the schema's
/// fields to actually resolve against real owner/tree data.
#[must_use]
pub fn assignment_fixture_with_sources(
    sources: SourceIndex,
    report: Report,
    active_mods: &[&str],
    refs: &[&str],
    targets: &[&str],
    schema: AssignmentSchema,
) -> (Session, AssignmentId) {
    let mut session = session_with_sources_and_mods(sources, report, active_mods);
    let (project, id) = new_test_assignment_project_with_schema(refs, targets, schema);
    session.upsert_assignment(project);
    (session, id)
}
