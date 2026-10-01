//! End-to-end: a **multi-section** assignment project through every real `rim-io`
//! adapter, against a scratch copy of the `assign_game` fixture (a
//! framework shipping five `example.PartAssignmentDef` instances — a
//! `speciesNames` target key, a `parts` item slot, a `chanceParts` field
//! paired to `parts` by name, and an `enabled` scalar — a parts mod
//! shipping the five `example.PartDef` items they reference, and a target
//! mod shipping five `ThingDef`s with a `<race>` marker — never the real
//! game install, never the real `ModsConfig.xml`). Proves, through the
//! real inference pipeline (`CreateAssignment::infer_candidate`, needing
//! `MIN_RESOLVED_DISTINCT = 5`, not a hand-built schema): a free-standing
//! ("new def") section created first, a target-keyed section added
//! afterward that references the free-standing section's own row via its
//! `ItemSlot` field, export renders both sections into one mod with no
//! self-dependency, and a re-scan hides the exported mod's own instances
//! while coverage after reload excludes its own now-active export.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_io::{
    AnalyzerScanner, FileDefSourceReader, JsonAssignmentProjectStore, JsonDecisionStore,
    JsonPatchProjectStore, JsonRuleStore, MergeModFolderWriter, ModsConfigFileStore,
};
use rim_resolve::domain::{AssignmentRow, DefKey, RowKey, RowValue, TargetRef};
use rim_session::use_cases::{
    AddAssignmentSection, AssignmentCoverage, AssignmentExportOptions, CreateAssignment,
    CreateAssignmentInput, ExportAssignment, LoadProject, SetAssignmentRow, UpdateAssignment,
    UpdateAssignmentInput,
};
use rim_session::{FindingFilter, ProjectPaths};
use tempfile::tempdir;

fn assign_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("assign_game")
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Builds a scratch `ProjectPaths` from a fresh copy of `assign_game`
/// under `root` — nothing outside the temp directory the caller owns is
/// ever touched.
fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
    let game_dir = root.join("game");
    copy_dir_recursive(&assign_game_fixture(), &game_dir)?;

    Ok(ProjectPaths {
        workshop_dir: root.join("workshop_does_not_exist"),
        mods_config: game_dir.join("ModsConfig.xml"),
        profile_dir: root.join("profile"),
        game_dir,
    })
}

fn load_project(paths: ProjectPaths) -> rim_session::Session {
    let use_case = LoadProject::new(
        AnalyzerScanner::new(),
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        JsonPatchProjectStore::new(),
        JsonAssignmentProjectStore::new(),
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    );
    use_case
        .execute(paths, &mut |_progress| {})
        .unwrap_or_else(|error| panic!("loading the scratch project must succeed: {error}"))
}

#[test]
fn create_a_free_standing_part_add_a_target_keyed_section_referencing_it_export_and_rescan() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");
    let mut session = load_project(paths.clone());

    let framework = ModId::new("fixture.framework");
    let parts_mod = ModId::new("fixture.parts");
    let target_mod = ModId::new("fixture.target");
    // Both `fixture.framework` and `fixture.parts` must be selected refs:
    // `fixture.framework` declares no `modDependencies` on `fixture.parts`,
    // so the effective-refs closure never pulls it in on its own.
    let refs: std::collections::BTreeSet<ModId> =
        [framework.clone(), parts_mod.clone()].into_iter().collect();

    // -- Phase 1: create a free-standing "new def" section for
    // `example.PartDef`, T empty (a no-`TargetKey` schema is only a valid
    // candidate when T is empty) -------------------------------------------

    let create_assignment = CreateAssignment::new(
        JsonAssignmentProjectStore::new(),
        FileDefSourceReader::new(),
    );
    let part_candidate = create_assignment
        .infer_candidate(
            &mut session,
            &refs,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
            "example.PartDef",
        )
        .expect("inferring the PartDef candidate must succeed")
        .expect("example.PartDef must be a valid standalone candidate");
    assert!(
        !part_candidate.schema.has_target_key(),
        "example.PartDef's own field (effect) never resolves outside R: {:?}",
        part_candidate.schema.fields
    );
    let assignment_id = create_assignment
        .execute(
            &mut session,
            CreateAssignmentInput {
                name: "Multi-section race patch".to_string(),
                package_id: "sample.multisection".to_string(),
                display_name: "Sample's Multi-Section Patch".to_string(),
                refs: refs.clone(),
                excluded_refs: std::collections::BTreeSet::new(),
                targets: std::collections::BTreeSet::new(),
                schema: part_candidate.schema,
            },
        )
        .expect("creating the standalone project must succeed");

    // A free-standing row of the new part: this project's own instance,
    // referenced by the target-keyed section added below.
    let set_row = SetAssignmentRow::new(JsonAssignmentProjectStore::new());
    set_row
        .execute(
            &mut session,
            &assignment_id,
            "example.PartDef",
            RowKey::Own("sample_multisection_OwnPart".to_string()),
            AssignmentRow {
                values: std::collections::BTreeMap::new(),
                def_name: "sample_multisection_OwnPart".to_string(),
                note: None,
            },
        )
        .expect("setting the free-standing part row must succeed");

    // -- Phase 2: give the project T = {fixture.target}, then add a
    // target-keyed `example.PartAssignmentDef` section — real inference, needing
    // the fixture's own 5 instances to clear `MIN_RESOLVED_DISTINCT` -------

    let update_assignment = UpdateAssignment::new(
        JsonAssignmentProjectStore::new(),
        FileDefSourceReader::new(),
    );
    update_assignment
        .execute(
            &mut session,
            &assignment_id,
            UpdateAssignmentInput {
                targets: Some([target_mod.clone()].into_iter().collect()),
                ..UpdateAssignmentInput::default()
            },
        )
        .expect("widening the target set must succeed");

    let add_section = AddAssignmentSection::new(
        JsonAssignmentProjectStore::new(),
        FileDefSourceReader::new(),
    );
    let race_group_candidate = add_section
        .execute(&mut session, &assignment_id, "example.PartAssignmentDef")
        .expect("adding the target-keyed section must succeed");
    assert!(
        race_group_candidate.schema.has_target_key(),
        "speciesNames must classify as a TargetKey field: {:?}",
        race_group_candidate.schema.fields
    );
    let project = session.assignment(&assignment_id).expect("still loaded");
    assert_eq!(project.sections().len(), 2, "{project:?}");

    // A target-keyed row for Race0, whose own `parts` slot names the
    // free-standing part set above — the whole point of this test.
    let race0_target = TargetRef {
        key_field: "speciesNames".parse().expect("valid path"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Race0".to_string(),
        },
    };
    set_row
        .execute(
            &mut session,
            &assignment_id,
            "example.PartAssignmentDef",
            RowKey::Target(race0_target.clone()),
            AssignmentRow {
                values: std::collections::BTreeMap::from([
                    (
                        "parts".parse().expect("valid path"),
                        RowValue::Names(vec!["sample_multisection_OwnPart".to_string()]),
                    ),
                    (
                        "chanceParts".parse().expect("valid path"),
                        RowValue::Numbers(vec![1.0]),
                    ),
                    (
                        "enabled".parse().expect("valid path"),
                        RowValue::Text("true".to_string()),
                    ),
                ]),
                def_name: "sample_multisection_Group_Race0".to_string(),
                note: None,
            },
        )
        .expect("setting the target-keyed row referencing the own part must succeed");

    // -- Coverage: Race0 is a candidate, `has_row` true (the framework's
    // own `Group_R0` — the same fixture data classification itself reads —
    // already references it too, so `matches` is non-empty independent of
    // this project) ----------------------------------------------------

    let coverage_use_case = AssignmentCoverage::new(FileDefSourceReader::new());
    let coverage_before_export = coverage_use_case
        .execute(&mut session, &assignment_id, "example.PartAssignmentDef")
        .expect("coverage must succeed");
    let race0_row = coverage_before_export
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Race0")
        .expect("Race0 must be a candidate");
    assert!(race0_row.has_row);

    // -- Export: both sections render into the same mod, install it -------

    let export_dir = scratch.path().join("export");
    let export_assignment = ExportAssignment::new(
        MergeModFolderWriter::new(),
        ModsConfigFileStore::new(),
        JsonAssignmentProjectStore::new(),
    );
    let outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_dir,
                install: true,
            },
        )
        .expect("exporting the multi-section project must succeed");
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    assert!(
        outcome
            .files
            .contains(&PathBuf::from("Defs/rimmerge_example.PartDef.xml")),
        "the free-standing section must render its own file: {:?}",
        outcome.files
    );
    assert!(
        outcome.files.contains(&PathBuf::from(
            "Defs/rimmerge_example.PartAssignmentDef.xml"
        )),
        "the target-keyed section must render its own file: {:?}",
        outcome.files
    );
    assert!(outcome.installed_path.is_some());

    let about_bytes =
        fs::read(outcome.export_path.join("About/About.xml")).expect("read About.xml");
    let about_text = String::from_utf8(about_bytes).expect("About.xml must be UTF-8");
    let deps_section = &about_text[about_text.find("<modDependencies>").unwrap()
        ..about_text.find("</modDependencies>").unwrap()];
    assert!(
        !deps_section.contains("sample.multisection"),
        "modDependencies must never name this project's own package id: {deps_section}"
    );

    let defs_xml = fs::read_to_string(
        outcome
            .export_path
            .join("Defs/rimmerge_example.PartAssignmentDef.xml"),
    )
    .expect("read the rendered PartAssignmentDef file");
    assert!(
        defs_xml.contains("sample_multisection_OwnPart"),
        "the row must name the free-standing part by its own defName: {defs_xml}"
    );

    // -- Re-scan: GeneratedMods must hide the installed assignment's own
    // instances (both sections') from every finding -------------------------

    let findings_before_reload = session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            limit: 200,
            ..FindingFilter::default()
        },
    );

    let mut second_session = load_project(paths);
    let assignment_mod = second_session
        .report()
        .mods
        .iter()
        .find(|m| m.id == ModId::new("sample.multisection"))
        .expect("the installed assignment must appear in the re-scanned report");
    assert_eq!(
        assignment_mod
            .generated
            .as_ref()
            .expect("the assignment folder must carry a rimmerge.json marker")
            .kind,
        rim_analyzer::domain::GeneratedKind::Assignment
    );

    let all_findings = second_session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            limit: 200,
            ..FindingFilter::default()
        },
    );
    assert!(
        all_findings
            .items
            .iter()
            .all(|key| !key.to_string().contains("sample.multisection")),
        "no finding may name the installed assignment: {all_findings:?}"
    );
    assert_eq!(
        all_findings.total, findings_before_reload.total,
        "installing and re-scanning must not change how many findings the profile ledger reports"
    );

    // -- Coverage after reload: this project's own now-active export must
    // never appear as an `ExistingMatch` on its own target -----------------

    let coverage_after_reload = coverage_use_case
        .execute(
            &mut second_session,
            &assignment_id,
            "example.PartAssignmentDef",
        )
        .expect("coverage must succeed after reload");
    let race0_after_reload = coverage_after_reload
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Race0")
        .expect("Race0 must still be a candidate after reload");
    assert!(
        race0_after_reload
            .matches
            .iter()
            .all(|existing| existing.owner.base() != ModId::new("sample.multisection")),
        "this project's own now-active export must never appear as an ExistingMatch: {race0_after_reload:?}"
    );
    assert_eq!(
        race0_after_reload.matches.len(),
        1,
        "only the framework's own pre-existing Group_R0 match should remain — this project's own \
         export excluded, never double-counted: {race0_after_reload:?}"
    );
    assert!(race0_after_reload.has_row);
}
