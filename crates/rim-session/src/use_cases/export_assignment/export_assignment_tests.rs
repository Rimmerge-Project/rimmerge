//! Tests for assignment export.

use std::collections::BTreeSet;
use std::sync::Arc;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{DefEntry, GeneratedMarker, ModId, Report, XmlLocator};
use rim_merge::emit::FileContent;
use rim_resolve::domain::{
    AssignmentSchema, Cardinality, DefKey, FieldSpec, OrderSource, RowValue, TargetRef,
};

use super::*;
use crate::ProjectPaths;
use crate::assignment_refs::SessionKnownDefs;
use crate::ports::ModsConfigFile;
use crate::test_support::{
    InMemoryAssignmentProjectStore, InMemoryMergeModWriter, InMemoryModsConfigStore,
    assignment_fixture_with_paths, assignment_fixture_with_sources, report_fixture,
};
use rim_resolve::domain::KnownDefs;
use rim_resolve::domain::{AssignmentProject, AssignmentRow, FieldRole, RowKey};
use std::collections::BTreeMap;

/// [`assignment_fixture_with_sources`]'s own fixed identity
/// (`"test.assignment"`/`"Test Assignment"`, folder `test_assignment`).
const TEST_ASSIGNMENT_PACKAGE_ID: &str = "test.assignment";
const TEST_ASSIGNMENT_FOLDER_NAME: &str = "test_assignment";

/// A [`KnownDefs`] fake answering "nothing is already active" — for a
/// row with no `ItemSlot` field, where the answer never matters.
struct NoneKnown;
impl KnownDefs for NoneKnown {
    fn contains(&self, _def_type: &str, _name: &str) -> bool {
        false
    }
}

fn schema() -> AssignmentSchema {
    let mut fields = BTreeMap::new();
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
    fields.insert(
        "parts".parse().unwrap(),
        FieldSpec {
            role: FieldRole::ItemSlot {
                def_type: "example.PartDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::from([ModId::new("fixture.framework")]),
        fields,
        target_shapes: BTreeMap::new(),
    }
}

fn locator(ordinal: u32) -> XmlLocator {
    XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
}

fn sources() -> SourceIndex {
    let mut index = SourceIndex::default();
    index.defs.insert(
        (
            ModId::new("fixture.target"),
            ("ThingDef".to_string(), "Elf".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Elf".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: locator(0),
        }],
    );
    index.owners_by_def.insert(
        ("ThingDef".to_string(), "Elf".to_string()),
        vec![ModId::new("fixture.target")],
    );
    index.defs_by_name.insert(
        "Wrench".to_string(),
        vec![(
            "example.PartDef".to_string(),
            ModId::new("fixture.framework"),
        )],
    );
    index.owners_by_def.insert(
        ("example.PartDef".to_string(), "Wrench".to_string()),
        vec![ModId::new("fixture.framework")],
    );
    index
}

fn report() -> Report {
    report_fixture(&["fixture.framework", "fixture.target"])
}

/// Every `<tag>...</tag>` value inside `text`, in document order — for
/// asserting an `About.xml` section's contents by *exact* list, not
/// `.contains`, since a substring check alone can't tell "present" from
/// "present and nothing else".
fn extract_tag_values<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let after_open = &rest[start + open.len()..];
        let Some(end) = after_open.find(&close) else {
            break;
        };
        values.push(&after_open[..end]);
        rest = &after_open[end + close.len()..];
    }
    values
}

fn config_store() -> InMemoryModsConfigStore {
    InMemoryModsConfigStore::new(ModsConfigFile {
        version: "1.6".to_string(),
        active_mods: vec![
            ModId::new("fixture.framework"),
            ModId::new("fixture.target"),
        ],
        known_expansions: Vec::new(),
    })
}

fn one_row_target() -> TargetRef {
    TargetRef {
        key_field: "speciesNames".parse().unwrap(),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Elf".to_string(),
        },
    }
}

fn one_row() -> AssignmentRow {
    AssignmentRow {
        values: BTreeMap::from([(
            "parts".parse().unwrap(),
            RowValue::Names(vec!["Wrench".to_string()]),
        )]),
        def_name: "mypatch_partassign_Elf".to_string(),
        note: None,
    }
}

/// A project over [`sources`]/[`report`], with one row naming `Elf`
/// (via `speciesNames`) and one item slot naming `Wrench` (owned by
/// `fixture.framework`) — enough to exercise a real
/// `modDependencies`/`loadAfter`/gate render end to end.
fn project_with_one_row() -> (Session, AssignmentId) {
    let (mut session, id) = assignment_fixture_with_sources(
        sources(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        session.assignment(&id).expect("still loaded"),
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(one_row_target()),
            one_row(),
            &known,
        )
        .expect("a valid row");
    session.upsert_assignment(project);
    (session, id)
}

fn exports_base() -> &'static Path {
    Path::new("C:/exports")
}

fn options(out_dir: &str) -> AssignmentExportOptions {
    AssignmentExportOptions {
        out_dir: exports_base().join(out_dir),
        install: false,
    }
}

#[allow(clippy::type_complexity)]
fn use_case(
    writer: InMemoryMergeModWriter,
    config: InMemoryModsConfigStore,
    store: InMemoryAssignmentProjectStore,
) -> ExportAssignment<InMemoryMergeModWriter, InMemoryModsConfigStore, InMemoryAssignmentProjectStore>
{
    ExportAssignment::new(writer, config, store)
}

#[test]
fn exports_a_project_with_one_row() {
    let (mut session, id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let outcome = use_case
        .execute(&mut session, &id, options("out"))
        .expect("export must succeed");

    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    assert!(outcome.files.contains(&PathBuf::from(
        "Defs/rimmerge_example.PartAssignmentDef.xml"
    )));
    assert!(outcome.files.contains(&PathBuf::from("About/About.xml")));
    assert!(outcome.files.contains(&PathBuf::from("rimmerge.json")));
    assert_eq!(
        outcome.export_path,
        exports_base().join("out").join(TEST_ASSIGNMENT_FOLDER_NAME)
    );
}

/// The dependency split: `fixture.framework` (this schema's own item owner, per
/// [`one_row`]'s `parts` value) lands in `modDependencies`, while
/// `fixture.target` (T, gated only via the row's own `MayRequire`)
/// lands in `loadAfter` alone.
#[test]
fn about_xml_declares_the_item_owner_in_moddependencies_and_target_only_in_loadafter() {
    let (mut session, id) = project_with_one_row();
    let writer = InMemoryMergeModWriter::new();
    let use_case = use_case(
        writer,
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    use_case
        .execute(&mut session, &id, options("out"))
        .expect("export must succeed");

    let rendered = use_case
        .writer
        .writes()
        .into_iter()
        .next()
        .expect("one write");
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .expect("About.xml must be rendered");
    let FileContent::Text(text) = &about.content else {
        panic!("About.xml must be text")
    };
    let deps_section =
        &text[text.find("<modDependencies>").unwrap()..text.find("</modDependencies>").unwrap()];
    let load_after_section =
        &text[text.find("<loadAfter>").unwrap()..text.find("</loadAfter>").unwrap()];
    assert_eq!(
        extract_tag_values(deps_section, "packageId"),
        vec!["fixture.framework"],
        "modDependencies must be exactly the item owner, never the target: {deps_section}"
    );
    assert_eq!(
        extract_tag_values(load_after_section, "li"),
        vec!["fixture.framework", "fixture.target"],
        "loadAfter must be exactly depends_on \u{222a} T: {load_after_section}"
    );
}

#[test]
fn exporting_twice_is_byte_identical() {
    let (mut session, id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let first = use_case
        .execute(&mut session, &id, options("out"))
        .expect("first export must succeed");
    let second = use_case
        .execute(&mut session, &id, options("out"))
        .expect("second export must succeed");

    assert_eq!(first.content_sha256, second.content_sha256);
    assert_eq!(first.files, second.files);
    assert_eq!(use_case.writer.writes().len(), 2);
}

/// A row's own item slot value can legitimately reference a since-
/// vanished item (its owning mod went inactive after the choice was
/// made) — reachable through [`AssignmentProject::from_stored`], which
/// trusts a stored row completely, exactly the shape
/// `rim_merge::assign`'s own role/value-mismatch tests reach those
/// mismatches through.
#[test]
fn a_vanished_item_is_skipped_and_omitted_not_shipped_stale() {
    // A `SourceIndex` that has never heard of `Wrench` at all — as if
    // `fixture.framework` (and its own `example.PartDef`) went
    // inactive since the row was chosen. `KnownDefs::contains` reads
    // `owners_by_def`, not `defs_by_name` — both must go for the
    // fixture to actually simulate "no longer known".
    let mut stale_sources = sources();
    stale_sources.defs_by_name.remove("Wrench");
    stale_sources
        .owners_by_def
        .remove(&("example.PartDef".to_string(), "Wrench".to_string()));
    let (mut session, id) = assignment_fixture_with_sources(
        stale_sources,
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
    );
    let project = session.assignment(&id).cloned().expect("loaded");
    let schema = project
        .section("example.PartAssignmentDef")
        .expect("the fixture's own section")
        .schema
        .clone();
    let stored = rim_resolve::domain::StoredAssignmentProject {
        id: project.id().clone(),
        name: project.name().to_string(),
        identity: project.identity().clone(),
        author: project.author().to_string(),
        description: project.description().to_string(),
        refs: project.refs().clone(),
        excluded_refs: project.excluded_refs().clone(),
        targets: project.targets().clone(),
        sections: BTreeMap::from([(
            schema.def_type.clone(),
            rim_resolve::domain::Section {
                schema,
                rows: BTreeMap::from([(RowKey::Target(one_row_target()), one_row())]),
            },
        )]),
        export_dir: None,
        created_at: project.created_at(),
        updated_at: project.updated_at(),
    };
    session.upsert_assignment(AssignmentProject::from_stored(stored));

    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );
    let outcome = use_case
        .execute(&mut session, &id, options("out"))
        .expect("export must still succeed with the field omitted");

    assert_eq!(outcome.skipped.len(), 1, "{:?}", outcome.skipped);
    assert!(outcome.skipped[0].reason.contains("Wrench"));
    let rendered = use_case
        .writer
        .writes()
        .into_iter()
        .next()
        .expect("one write");
    let defs_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartAssignmentDef.xml"))
        .expect("the Defs/ file must still render");
    let FileContent::Text(text) = &defs_file.content else {
        panic!("Defs/ file must be text")
    };
    assert!(
        !text.contains("Wrench"),
        "the vanished item must never appear in the rendered instance: {text}"
    );
}

#[test]
fn refuses_a_relative_out_dir() {
    let (mut session, id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let result = use_case.execute(
        &mut session,
        &id,
        AssignmentExportOptions {
            out_dir: PathBuf::from("Mods/x"),
            install: false,
        },
    );

    assert!(matches!(
        result,
        Err(ExportAssignmentError::OutDirNotAbsolute(_))
    ));
    assert!(use_case.writer.writes().is_empty());
}

fn absolute_test_paths() -> ProjectPaths {
    ProjectPaths {
        game_dir: PathBuf::from("C:/game"),
        workshop_dir: PathBuf::from("C:/workshop"),
        mods_config: PathBuf::from("C:/game/ModsConfig.xml"),
        profile_dir: PathBuf::from("C:/profile"),
    }
}

/// Mirrors `export_patch.rs`'s own
/// `refuses_an_out_dir_inside_the_games_mods_folder`: needs an
/// *absolute* `game_dir` (unlike every other fixture's relative
/// `"game"`), since `options.out_dir` must be absolute too.
#[test]
fn refuses_an_out_dir_inside_the_games_mods_folder() {
    let (mut session, id) = assignment_fixture_with_paths(
        sources(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
        absolute_test_paths(),
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        session.assignment(&id).expect("still loaded"),
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(one_row_target()),
            one_row(),
            &known,
        )
        .expect("a valid row");
    session.upsert_assignment(project);
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );
    let mods_subfolder = session.paths().game_dir.join("Mods").join("some_folder");

    let result = use_case.execute(
        &mut session,
        &id,
        AssignmentExportOptions {
            out_dir: mods_subfolder,
            install: false,
        },
    );

    assert!(matches!(
        result,
        Err(ExportAssignmentError::OutDirIsModsFolder(_))
    ));
    assert!(use_case.writer.writes().is_empty());
}

#[test]
fn re_exporting_its_own_previous_folder_succeeds() {
    let (mut session, id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    use_case
        .execute(&mut session, &id, options("out"))
        .expect("first export must succeed");
    let second = use_case.execute(&mut session, &id, options("out"));

    assert!(second.is_ok(), "{second:?}");
    assert_eq!(use_case.writer.writes().len(), 2);
}

#[test]
fn refuses_to_overwrite_a_foreign_folder() {
    let (mut session, id) = project_with_one_row();
    let writer = InMemoryMergeModWriter::new();
    writer.set_marker(
        TEST_ASSIGNMENT_FOLDER_NAME,
        GeneratedMarker {
            kind: GeneratedKind::Patch,
            patch_id: Some("abcdef012345".to_string()),
            scope: Some(BTreeSet::new()),
        },
    );
    let use_case = use_case(
        writer,
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let result = use_case.execute(&mut session, &id, options("out"));

    assert!(matches!(
        result,
        Err(ExportAssignmentError::ForeignFolder { .. })
    ));
}

#[test]
fn installing_copies_into_mods_and_appends_the_package_id() {
    let (mut session, id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let outcome = use_case
        .execute(
            &mut session,
            &id,
            AssignmentExportOptions {
                out_dir: exports_base().join("out"),
                install: true,
            },
        )
        .expect("export with install must succeed");

    assert!(outcome.installed_path.is_some());
    assert!(outcome.mods_config_backup.is_some());
    assert!(
        session
            .orders()
            .current
            .as_slice()
            .contains(&ModId::new(TEST_ASSIGNMENT_PACKAGE_ID))
    );
    assert_eq!(session.selected(), OrderSource::Current);
}

#[test]
fn a_failed_save_rolls_back_the_export_dir_but_keeps_the_written_files() {
    let (mut session, id) = project_with_one_row();
    let store = InMemoryAssignmentProjectStore::new();
    store.fail_next_save();
    let writer = InMemoryMergeModWriter::new();
    let use_case = use_case(writer, config_store(), store);

    let result = use_case.execute(&mut session, &id, options("out"));

    assert!(matches!(result, Err(ExportAssignmentError::Store(_))));
    assert!(
        session
            .assignment(&id)
            .expect("still loaded")
            .export_dir()
            .is_none(),
        "the export_dir mutation must be rolled back"
    );
    assert_eq!(
        use_case.writer.writes().len(),
        1,
        "the file write itself is never rolled back"
    );
}

#[test]
fn rejects_an_unknown_assignment() {
    let (mut session, _id) = project_with_one_row();
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );
    let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

    let result = use_case.execute(&mut session, &bogus, options("out"));

    assert!(matches!(result, Err(ExportAssignmentError::Unknown(_))));
}

/// A standalone ("new def") project — no `TargetKey` field — exports a
/// free-standing row with no `MayRequire`, and its `modDependencies`
/// comes from the item owner alone (never a target, since it has
/// none).
#[test]
fn exports_a_standalone_project_with_no_may_require() {
    let mut standalone_schema = schema();
    standalone_schema.def_type = "example.PartDef2".to_string();
    standalone_schema
        .fields
        .remove(&"speciesNames".parse().unwrap());
    let (mut session, id) = assignment_fixture_with_sources(
        sources(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &[],
        standalone_schema,
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        session.assignment(&id).expect("still loaded"),
    );
    project
        .set_row(
            "example.PartDef2",
            RowKey::Own("sample_newpart_Base".to_string()),
            AssignmentRow {
                values: BTreeMap::from([(
                    "parts".parse().unwrap(),
                    RowValue::Names(vec!["Wrench".to_string()]),
                )]),
                def_name: "sample_newpart_Base".to_string(),
                note: None,
            },
            &known,
        )
        .expect("a valid standalone row");
    session.upsert_assignment(project);
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let outcome = use_case
        .execute(&mut session, &id, options("out"))
        .expect("export must succeed");

    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    let rendered = use_case
        .writer
        .writes()
        .into_iter()
        .next()
        .expect("one write");
    let defs_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartDef2.xml"))
        .expect("the Defs/ file must render");
    let FileContent::Text(text) = &defs_file.content else {
        panic!("Defs/ file must be text")
    };
    assert!(text.contains("<defName>sample_newpart_Base</defName>"));
    assert!(
        !text.contains("MayRequire"),
        "a standalone row has no target to gate against: {text}"
    );
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .expect("About.xml must be rendered");
    let FileContent::Text(about_text) = &about.content else {
        panic!("About.xml must be text")
    };
    assert!(
        about_text.contains("fixture.framework"),
        "modDependencies must still name the item owner: {about_text}"
    );
}

/// Two sections — one
/// target-keyed (`example.PartAssignmentDef`), one free-standing
/// (`example.PartDef`) — export into the same mod, the target-keyed
/// row's own item slot names the free-standing section's own row
/// *alongside* a genuinely active external instance of the same item
/// type (`Wrench`, owned by `fixture.framework` per this module's own
/// `sources()`), and `modDependencies` never contains this project's
/// own package id. Exporting twice is byte-identical.
///
/// **Regression coverage**: a free-standing section coexisting with a
/// reference to an *external* instance of the same item type must not
/// silently skip the external name at export ("item(s) Wrench no longer
/// exist as a known own instance of example.PartDef"), as a guard
/// treating any project section for that type as grounds to reject every
/// reference that isn't itself an own instance would. `Wrench` is the
/// value that exercises this, and `assert!(first.skipped.is_empty())`
/// below is the regression assertion.
#[test]
fn exports_two_sections_with_a_cross_section_item_reference_and_no_self_dependency() {
    let (mut session, id) = assignment_fixture_with_sources(
        sources(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    project
        .add_section(AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        })
        .expect("a fresh def type must add cleanly");
    // No `ItemSlot` field on this free-standing section's own schema,
    // so `KnownDefs` never matters for this first row.
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("OwnPart".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "OwnPart".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("a valid free-standing row");
    // Read `known` off a clone taken *after* the row above, so
    // `own_instances` sees `OwnPart` — a plain clone rather than a
    // borrow of `project` itself, which is about to be mutated again.
    let with_own_part = project.clone();
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        &with_own_part,
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(one_row_target()),
            AssignmentRow {
                values: BTreeMap::from([(
                    "parts".parse().unwrap(),
                    // `Wrench` alongside `OwnPart`: a genuinely active
                    // *external* `example.PartDef` instance (owned by
                    // `fixture.framework` per this module's own
                    // `sources()`), not merely another own reference —
                    // the shape an own-instance-only guard would wrongly
                    // reject (see this test's own doc comment).
                    RowValue::Names(vec!["OwnPart".to_string(), "Wrench".to_string()]),
                )]),
                def_name: "mypatch_partassign_Elf".to_string(),
                note: None,
            },
            &known,
        )
        .expect("a row referencing the free-standing part and an active external instance");
    session.upsert_assignment(project);
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let first = use_case
        .execute(&mut session, &id, options("out"))
        .expect("first export must succeed");
    let second = use_case
        .execute(&mut session, &id, options("out"))
        .expect("second export must succeed");

    assert!(first.skipped.is_empty(), "{:?}", first.skipped);
    assert_eq!(first.content_sha256, second.content_sha256);
    assert_eq!(first.files, second.files);
    assert!(first.files.contains(&PathBuf::from(
        "Defs/rimmerge_example.PartAssignmentDef.xml"
    )));
    assert!(
        first
            .files
            .contains(&PathBuf::from("Defs/rimmerge_example.PartDef.xml"))
    );
    let rendered = use_case
        .writer
        .writes()
        .into_iter()
        .next()
        .expect("one write");
    let race_group_defs = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartAssignmentDef.xml"))
        .expect("the PartAssignmentDef Defs/ file must be rendered");
    let FileContent::Text(race_group_text) = &race_group_defs.content else {
        panic!("Defs/ file must be text")
    };
    assert!(
        race_group_text.contains("OwnPart"),
        "the own reference must still render: {race_group_text}"
    );
    assert!(
        race_group_text.contains("Wrench"),
        "the active external reference must render alongside the own one, not be skipped: \
             {race_group_text}"
    );
    let about = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("About/About.xml"))
        .expect("About.xml must be rendered");
    let FileContent::Text(about_text) = &about.content else {
        panic!("About.xml must be text")
    };
    // `about_text` legitimately names its own package id once, in its
    // own `<packageId>` tag — the assertion is scoped to
    // `<modDependencies>`/`<loadAfter>` alone, not the whole document.
    let deps_section = &about_text[about_text.find("<modDependencies>").unwrap()
        ..about_text.find("</modDependencies>").unwrap()];
    let load_after_section = &about_text
        [about_text.find("<loadAfter>").unwrap()..about_text.find("</loadAfter>").unwrap()];
    // `Wrench`'s own owner, `fixture.framework`, is a genuine
    // `modDependencies` entry the own-package exclusion must skip
    // *past* — proving that exclusion is selective, not vacuous.
    assert!(
        deps_section.contains("fixture.framework"),
        "modDependencies must name Wrench's own owner: {deps_section}"
    );
    assert!(
        !deps_section.contains(TEST_ASSIGNMENT_PACKAGE_ID)
            && !load_after_section.contains(TEST_ASSIGNMENT_PACKAGE_ID),
        "modDependencies/loadAfter must never name this project's own package id: {about_text}"
    );
}

/// Once a section referenced by another section's `ItemSlot` value is
/// force-removed, the dangling reference is skipped (never shipped
/// silently) at export time.
#[test]
fn a_reference_to_a_force_removed_sections_own_row_is_skipped_with_a_reason() {
    let (mut session, id) = assignment_fixture_with_sources(
        sources(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    project
        .add_section(AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        })
        .expect("a fresh def type must add cleanly");
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("OwnPart".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "OwnPart".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("a valid free-standing row");
    let with_own_part = project.clone();
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        &with_own_part,
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(one_row_target()),
            AssignmentRow {
                values: BTreeMap::from([(
                    "parts".parse().unwrap(),
                    RowValue::Names(vec!["OwnPart".to_string()]),
                )]),
                def_name: "mypatch_partassign_Elf".to_string(),
                note: None,
            },
            &known,
        )
        .expect("a row referencing the free-standing part");
    // Force-remove the referenced section directly (the domain-level
    // operation `RemoveAssignmentSection` itself wraps) — the
    // referencing row's own value is left dangling on purpose.
    project
        .remove_section("example.PartDef", true)
        .expect("force must remove regardless");
    session.upsert_assignment(project);
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let outcome = use_case
        .execute(&mut session, &id, options("out"))
        .expect("export must still succeed with the field omitted");

    assert_eq!(outcome.skipped.len(), 1, "{:?}", outcome.skipped);
    assert_eq!(outcome.skipped[0].def_type, "example.PartAssignmentDef");
    assert!(
        outcome.skipped[0].reason.contains("OwnPart"),
        "{:?}",
        outcome.skipped[0]
    );
    let rendered = use_case
        .writer
        .writes()
        .into_iter()
        .next()
        .expect("one write");
    let defs_file = rendered
        .files
        .iter()
        .find(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartAssignmentDef.xml"))
        .expect("the referencing section's own file must still render");
    let FileContent::Text(text) = &defs_file.content else {
        panic!("Defs/ file must be text")
    };
    assert!(
        !text.contains("OwnPart"),
        "the dangling reference must never appear in the rendered instance: {text}"
    );
    assert!(
        !rendered
            .files
            .iter()
            .any(|f| f.relative_path == Path::new("Defs/rimmerge_example.PartDef.xml")),
        "the force-removed section must render no file of its own"
    );
}

/// End to end: an empty `SourceIndex` means `Elf` has no active
/// owner at all, so [`build_gates`] records no entry for it, and
/// `render_rows` skips the whole (only) row rather than rendering it
/// ungated — nothing left to write.
#[test]
fn nothing_to_export_when_the_only_row_has_no_active_owner() {
    let (mut session, id) = assignment_fixture_with_sources(
        SourceIndex::default(),
        report(),
        &["fixture.framework", "fixture.target"],
        &["fixture.framework"],
        &["fixture.target"],
        schema(),
    );
    let mut project = session.assignment(&id).cloned().expect("loaded");
    // `set_row` itself validates an item slot's names against `known`
    // (empty here, since the index is empty), so this row omits the
    // item field entirely — irrelevant to what this test exercises
    // (the missing-gate skip, not the vanished-item one).
    let known = SessionKnownDefs::new(
        &session,
        project.identity().package_id().clone(),
        session.assignment(&id).expect("still loaded"),
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(one_row_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_partassign_Elf".to_string(),
                note: None,
            },
            &known,
        )
        .expect("a valid row");
    session.upsert_assignment(project);
    let use_case = use_case(
        InMemoryMergeModWriter::new(),
        config_store(),
        InMemoryAssignmentProjectStore::new(),
    );

    let result = use_case.execute(&mut session, &id, options("out"));

    assert!(matches!(
        result,
        Err(ExportAssignmentError::NothingToExport)
    ));
}
