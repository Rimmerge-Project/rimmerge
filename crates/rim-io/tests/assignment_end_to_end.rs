//! End-to-end: `LoadProject` -> `CreateAssignment` -> `SetAssignmentRow` ->
//! `AssignmentCoverage` -> `ExportAssignment` (once plain, once with
//! `install: true`) -> `LoadProject` again, wired to every real `rim-io`
//! adapter, against a scratch copy of the `assign_export_game` fixture
//! (`Framework` ships `example.PartDef Wrench`, `Target` ships
//! `ThingDef Elf`/`Dwarf`, `Existing` already ships a `example.PartAssignmentDef`
//! naming `Dwarf`) — never the real game install, never the real
//! `ModsConfig.xml`. Proves the whole patch-maker export pipeline (scan ->
//! create -> set row -> coverage -> export -> install -> re-scan)
//! round-trips together.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::{GameVersion, GeneratedKind, ModId};
use rim_analyzer::extract::{about_xml, rimmerge_marker};
use rim_io::{
    AnalyzerScanner, FileDefSourceReader, JsonAssignmentProjectStore, JsonDecisionStore,
    JsonPatchProjectStore, JsonRuleStore, MergeModFolderWriter, ModsConfigFileStore,
};
use rim_resolve::domain::{
    AssignmentSchema, Cardinality, DefKey, FieldRole, FieldSpec, RowIntent, RowValue, TargetRef,
};
use rim_session::use_cases::{
    AssignmentCoverage, AssignmentExportOptions, CreateAssignment, CreateAssignmentError,
    CreateAssignmentInput, ExportAssignment, LoadProject, SetAssignmentRow,
};
use rim_session::{FindingFilter, ProjectPaths};
use tempfile::tempdir;

fn assign_export_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("assign_export_game")
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

/// Builds a scratch `ProjectPaths` from a fresh copy of `assign_export_game`
/// under `root` — nothing outside the temp directory the caller owns is
/// ever touched.
fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
    let game_dir = root.join("game");
    copy_dir_recursive(&assign_export_game_fixture(), &game_dir)?;

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

/// The one-field-plus-one-slot schema this test's whole pipeline exercises
/// — hand-built rather than inferred through `CreateAssignment::propose`
/// (whose thresholds need a much larger fixture than this scratch game
/// needs to prove the export/coverage/rescan pipeline itself):
/// `speciesNames` (`FieldRole::TargetKey<ThingDef>`) and `parts`
/// (`FieldRole::ItemSlot<example.PartDef>`).
fn schema() -> AssignmentSchema {
    let mut fields = BTreeMap::new();
    fields.insert(
        "speciesNames"
            .parse()
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
    fields.insert(
        "parts"
            .parse()
            .unwrap_or_else(|error| unreachable!("a fixed, valid field path: {error}")),
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
        refs: [ModId::new("export.framework")].into_iter().collect(),
        fields,
        target_shapes: BTreeMap::new(),
    }
}

fn elf_target() -> TargetRef {
    TargetRef {
        key_field: "speciesNames"
            .parse()
            .unwrap_or_else(|error| unreachable!("a fixed, valid field path: {error}")),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Elf".to_string(),
        },
    }
}

/// Every file under `dir`, as `(relative path, content bytes)`, sorted for
/// a deterministic byte-for-byte comparison across two independent exports
/// of the same folder.
fn snapshot_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}")) {
            let entry = entry.unwrap_or_else(|e| panic!("dir entry under {dir:?}: {e}"));
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or_else(|e| panic!("{path:?} must be under {root:?}: {e}"))
                    .to_path_buf();
                let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
                out.push((relative, bytes));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn create_set_row_cover_export_and_install_an_assignment_round_trips() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");
    let mut session = load_project(paths.clone());

    let framework = ModId::new("export.framework");
    let target = ModId::new("export.target");

    // -- CreateAssignment, R = {framework}, T = {target} ------------------

    let create_assignment = CreateAssignment::new(
        JsonAssignmentProjectStore::new(),
        FileDefSourceReader::new(),
    );
    let assignment_id = create_assignment
        .execute(
            &mut session,
            CreateAssignmentInput {
                name: "Race Assignments".to_string(),
                package_id: "mypatch.partassign".to_string(),
                display_name: "Race Assignments".to_string(),
                refs: [framework.clone()].into_iter().collect(),
                excluded_refs: std::collections::BTreeSet::new(),
                targets: [target.clone()].into_iter().collect(),
                schema: schema(),
            },
        )
        .expect("creating the assignment must succeed");

    // -- Coverage before this project has any row: both Elf and Dwarf are
    // candidates, Dwarf already has an existing match from `Existing`. ---

    let coverage_use_case = AssignmentCoverage::new(FileDefSourceReader::new());
    let coverage_before = coverage_use_case
        .execute(&mut session, &assignment_id, "example.PartAssignmentDef")
        .expect("coverage must succeed");
    assert_eq!(coverage_before.rows.len(), 2, "{coverage_before:?}");
    let dwarf_before = coverage_before
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Dwarf")
        .expect("Dwarf must be a candidate");
    assert_eq!(dwarf_before.intent, RowIntent::Override);
    assert_eq!(dwarf_before.matches.len(), 1);
    assert_eq!(dwarf_before.matches[0].owner, ModId::new("export.existing"));
    assert_eq!(dwarf_before.matches[0].instance_def_name, "Group_Dwarf");
    let elf_before = coverage_before
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Elf")
        .expect("Elf must be a candidate");
    assert_eq!(elf_before.intent, RowIntent::Cover);
    assert!(!elf_before.has_row);

    // -- SetAssignmentRow: Elf, naming the framework's own part ------------

    let set_row = SetAssignmentRow::new(JsonAssignmentProjectStore::new());
    set_row
        .execute(
            &mut session,
            &assignment_id,
            "example.PartAssignmentDef",
            rim_resolve::domain::RowKey::Target(elf_target()),
            rim_resolve::domain::AssignmentRow {
                values: BTreeMap::from([(
                    "parts".parse().expect("valid field path"),
                    RowValue::Names(vec!["Wrench".to_string()]),
                )]),
                def_name: "mypatch_partassign_Elf".to_string(),
                note: None,
            },
        )
        .expect("setting the row must succeed");

    let coverage_after = coverage_use_case
        .execute(&mut session, &assignment_id, "example.PartAssignmentDef")
        .expect("coverage must succeed");
    let elf_after = coverage_after
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Elf")
        .expect("Elf must still be a candidate");
    assert!(
        elf_after.has_row,
        "the cache must reflect the just-set row, not a stale pre-row result"
    );
    assert_eq!(
        elf_after.intent,
        RowIntent::Cover,
        "no *existing* instance references Elf yet"
    );

    // -- ExportAssignment to a plain temp directory ------------------------

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
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .expect("exporting the assignment must succeed");
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    assert!(outcome.installed_path.is_none());
    assert!(outcome.mods_config_backup.is_none());

    let export_path = outcome.export_path.clone();
    assert!(export_path.is_dir(), "the exported folder must exist");
    assert!(export_path.join("About/About.xml").is_file());
    assert!(export_path.join("rimmerge.json").is_file());
    assert!(
        export_path
            .join("Defs/rimmerge_example.PartAssignmentDef.xml")
            .is_file(),
        "the row must produce a Defs/ file"
    );

    let defs_xml =
        fs::read_to_string(export_path.join("Defs/rimmerge_example.PartAssignmentDef.xml"))
            .expect("read the rendered Defs/ file");
    assert!(defs_xml.contains("mypatch_partassign_Elf"));
    assert!(defs_xml.contains("Wrench"));
    assert!(
        defs_xml.contains(r#"MayRequire="export.target""#),
        "Elf's own row must be gated on its target's mod: {defs_xml}"
    );

    // -- About.xml: the modDependencies/loadAfter split -------------------

    let about_bytes = fs::read(export_path.join("About/About.xml")).expect("read About.xml");
    let about =
        about_xml::parse(&about_bytes, GameVersion::new(1, 6)).expect("About.xml must parse");
    let declared_deps: Vec<ModId> = about
        .declared
        .dependencies
        .iter()
        .map(|dependency| dependency.id.clone())
        .collect();
    assert_eq!(
        declared_deps,
        vec![framework.clone()],
        "modDependencies must be exactly the item owner (the framework), never the target"
    );
    let declared_load_after = about.declared.load_after.clone();
    assert!(declared_load_after.contains(&framework));
    assert!(
        declared_load_after.contains(&target),
        "loadAfter must also carry T (the target-only mod): {declared_load_after:?}"
    );

    let marker_bytes = fs::read(export_path.join("rimmerge.json")).expect("read rimmerge.json");
    let marker =
        rimmerge_marker::parse(&marker_bytes).expect("rimmerge.json must parse as a marker");
    assert_eq!(marker.kind, GeneratedKind::Assignment);
    assert_eq!(marker.patch_id.as_deref(), Some(assignment_id.as_str()));
    let scope = marker
        .scope
        .expect("an assignment marker always carries a scope");
    assert!(scope.contains(&framework));
    assert!(scope.contains(&target));

    // -- Exporting again must be byte-identical ---------------------------

    let first_export_snapshot = snapshot_files(&export_path);
    let second_outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .expect("exporting a second time must succeed");
    assert_eq!(second_outcome.export_path, export_path);
    let second_export_snapshot = snapshot_files(&export_path);
    assert_eq!(
        first_export_snapshot, second_export_snapshot,
        "exporting the same project twice must be byte-identical"
    );

    let prev_backup = paths
        .profile_dir
        .join("assignments")
        .join(format!("{assignment_id}.prev"));
    assert!(
        prev_backup.join("About/About.xml").is_file(),
        "the second export must back up the first generation under assignments/<id>.prev"
    );

    // Baseline, captured before install, to prove installing (and the
    // re-scan below) only ever adds a hidden entry for the new mod itself.
    let findings_before_install = session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            limit: 200,
            ..FindingFilter::default()
        },
    );

    // -- ExportAssignment { install: true } --------------------------------

    let install_outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_dir,
                install: true,
            },
        )
        .expect("exporting with install must succeed");
    let installed_path = install_outcome
        .installed_path
        .expect("install must produce an installed path");
    assert!(installed_path.is_dir());
    assert_eq!(
        installed_path,
        paths.game_dir.join("Mods").join("mypatch_partassign")
    );
    assert!(install_outcome.mods_config_backup.is_some());

    let rewritten_mods_config =
        fs::read_to_string(&paths.mods_config).expect("re-read scratch ModsConfig.xml");
    let assignment_position = rewritten_mods_config
        .find("<li>mypatch.partassign</li>")
        .unwrap_or_else(|| {
            panic!("must list the assignment's package id: {rewritten_mods_config}")
        });
    for other in ["<li>export.framework</li>", "<li>export.target</li>"] {
        let other_position = rewritten_mods_config
            .find(other)
            .unwrap_or_else(|| panic!("must still list {other}: {rewritten_mods_config}"));
        assert!(
            other_position < assignment_position,
            "the assignment's package id must be appended last, after {other}: {rewritten_mods_config}"
        );
    }

    // -- Re-scan: GeneratedMods must hide the installed assignment's own
    // instance from every finding ------------------------------------------

    let mut second_session = load_project(paths.clone());
    let assignment_mod = second_session
        .report()
        .mods
        .iter()
        .find(|m| m.id == ModId::new("mypatch.partassign"))
        .expect("the installed assignment must appear in the re-scanned report");
    let generated = assignment_mod
        .generated
        .as_ref()
        .expect("the assignment folder must carry a rimmerge.json marker");
    assert_eq!(generated.kind, GeneratedKind::Assignment);
    assert_eq!(generated.patch_id.as_deref(), Some(assignment_id.as_str()));

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
            .all(|key| !key.to_string().contains("mypatch.partassign")),
        "no finding may name the installed assignment: {all_findings:?}"
    );
    assert_eq!(
        all_findings.total, findings_before_install.total,
        "installing and re-scanning the assignment must not change how many findings the profile ledger reports"
    );

    // -- Coverage after reload: this project's own installed instance must
    // never show up twice under two different identities -------------------

    assert!(
        second_session.assignment(&assignment_id).is_some(),
        "the assignment project itself must survive a reload from disk"
    );
    let coverage_after_reload = coverage_use_case
        .execute(
            &mut second_session,
            &assignment_id,
            "example.PartAssignmentDef",
        )
        .expect("coverage must succeed after reload");
    let elf_after_reload = coverage_after_reload
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Elf")
        .expect("Elf must still be a candidate after reload");
    assert!(
        elf_after_reload.matches.is_empty(),
        "this project's own now-active export must never appear as an ExistingMatch: {elf_after_reload:?}"
    );
    assert!(elf_after_reload.has_row);
    let dwarf_after_reload = coverage_after_reload
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Dwarf")
        .expect("Dwarf must still be a candidate after reload");
    assert_eq!(
        dwarf_after_reload.matches.len(),
        1,
        "{dwarf_after_reload:?}"
    );
}

/// The "growing R re-infers"/"shrinking T drops rows" tests
/// are `rim-session`'s own unit tests (`use_cases::update_assignment`); this
/// file only proves `CreateAssignment` rejects an empty ref/target set
/// against the *real* adapters too, matching `patch_end_to_end.rs`'s own
/// division of labor between unit and integration coverage.
#[test]
fn create_assignment_rejects_an_empty_target_set_against_the_real_store() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");
    let mut session = load_project(paths);
    let create_assignment = CreateAssignment::new(
        JsonAssignmentProjectStore::new(),
        FileDefSourceReader::new(),
    );

    let result = create_assignment.execute(
        &mut session,
        CreateAssignmentInput {
            name: "n".to_string(),
            package_id: "mypatch.partassign".to_string(),
            display_name: "d".to_string(),
            refs: [ModId::new("export.framework")].into_iter().collect(),
            excluded_refs: std::collections::BTreeSet::new(),
            targets: std::collections::BTreeSet::new(),
            schema: schema(),
        },
    );

    assert_eq!(result, Err(CreateAssignmentError::EmptyTargets));
}
