//! End-to-end: `LoadProject` -> `CreatePatch` -> `DecidePatchMerge` ->
//! `ExportPatch` (once plain, once with `install: true`) -> `LoadProject`
//! again, wired to every real `rim-io` adapter, against a scratch copy of
//! `merge_end_to_end.rs`'s own `merge_game` fixture (`ModA`/`ModB`, both
//! defining `ThingDef/Fixture_Wall` with differing `statBases/MaxHitPoints`)
//! — never the real game install, never the real `ModsConfig.xml`. Proves
//! the whole compat-patch pipeline (scan -> create -> decide -> export ->
//! install -> re-scan) round-trips together.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::{GameVersion, GeneratedKind, ModId};
use rim_analyzer::extract::{about_xml, rimmerge_marker};
use rim_io::{
    AnalyzerScanner, FileAssetLocator, FileDefSourceReader, JsonAssignmentProjectStore,
    JsonDecisionStore, JsonPatchProjectStore, JsonRuleStore, MergeModFolderWriter,
    ModsConfigFileStore,
};
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, AssignmentSchema, FieldPath, MergeChoice, PatchModIdentity,
};
use rim_session::ports::AssignmentProjectStore;
use rim_session::use_cases::{
    CreatePatch, CreatePatchInput, DecidePatchMerge, ExportOptions, ExportPatch, ExportPatchError,
    LoadProject,
};
use rim_session::{FindingFilter, FindingKind, ProjectPaths};
use tempfile::tempdir;

fn merge_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("merge_game")
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

/// Builds a scratch `ProjectPaths` from a fresh copy of `merge_game` under
/// `root` — nothing outside the temp directory the caller owns is ever
/// touched.
fn scratch_paths(root: &Path) -> std::io::Result<ProjectPaths> {
    let game_dir = root.join("game");
    copy_dir_recursive(&merge_game_fixture(), &game_dir)?;

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

/// Every file under `dir`, as `(relative path, content bytes)`, sorted for
/// a deterministic byte-for-byte comparison across two independent scans
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
fn create_decide_export_and_install_a_compat_patch_round_trips() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");

    // -- The real JsonAssignmentProjectStore round-trips alongside a real
    //    profile scan, the same way JsonPatchProjectStore already does
    //    below -------------------------------------------------------
    let assignment_identity =
        PatchModIdentity::new("mypatch.partassign", "Race Assignment").expect("valid identity");
    let assignment_id = AssignmentId::derive(
        paths.profile_hash(),
        assignment_identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let assignment_project = AssignmentProject::new(
        assignment_id.clone(),
        "Race assignment".to_string(),
        assignment_identity,
        [ModId::new("fixture.moda")].into_iter().collect(),
        [ModId::new("fixture.modb")].into_iter().collect(),
        AssignmentSchema {
            def_type: "ThingDef".to_string(),
            refs: std::collections::BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        },
        jiff::Timestamp::UNIX_EPOCH,
    );
    JsonAssignmentProjectStore::new()
        .save(&paths.profile_dir, &assignment_project)
        .expect("seeding the real assignment store must succeed");

    let mut session = load_project(paths.clone());
    assert!(
        session.assignment(&assignment_id).is_some(),
        "the assignment project must load through the real store alongside the real scan"
    );

    let mod_a = ModId::new("fixture.moda");
    let mod_b = ModId::new("fixture.modb");

    // -- CreatePatch, scoped to ModA + ModB -------------------------------

    let create_patch = CreatePatch::new(JsonPatchProjectStore::new());
    let patch_id = create_patch
        .execute(
            &mut session,
            CreatePatchInput {
                name: "AB compat".to_string(),
                package_id: "sample.abcompat".to_string(),
                display_name: "A + B Compatibility".to_string(),
                scope: [mod_a.clone(), mod_b.clone()].into_iter().collect(),
            },
        )
        .expect("creating the patch must succeed");

    // -- DecidePatchMerge: pick ModA's MaxHitPoints -----------------------

    let def_override_page = session.patch_findings(
        &patch_id,
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            kinds: Some(vec![FindingKind::DefOverride]),
            limit: 10,
            ..FindingFilter::default()
        },
    );
    let def_override_page =
        def_override_page.unwrap_or_else(|error| panic!("patch_findings must succeed: {error}"));
    assert_eq!(
        def_override_page.total, 1,
        "the fixture's ModA/ModB must produce exactly one scoped DefOverride finding"
    );
    let key = def_override_page.items[0].clone();

    let mut choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::new();
    choices.insert(
        "statBases/MaxHitPoints".parse().expect("valid field path"),
        MergeChoice::From {
            mod_id: mod_a.clone(),
        },
    );
    let decide_patch_merge =
        DecidePatchMerge::new(JsonPatchProjectStore::new(), FileDefSourceReader::new());
    let state = decide_patch_merge
        .execute(&mut session, &patch_id, &key, choices)
        .expect("deciding the patch merge must succeed");
    assert!(
        matches!(state, rim_resolve::domain::MergeState::Complete { .. }),
        "picking ModA's own value must resolve every field: {state:?}"
    );

    // -- ExportPatch to a plain temp directory ----------------------------

    let export_dir = scratch.path().join("export");
    let export_patch = ExportPatch::new(
        MergeModFolderWriter::new(),
        FileDefSourceReader::new(),
        FileAssetLocator::new(),
        ModsConfigFileStore::new(),
        JsonPatchProjectStore::new(),
    );
    let outcome = export_patch
        .execute(
            &mut session,
            &patch_id,
            ExportOptions {
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .expect("exporting the patch must succeed");
    assert!(outcome.skipped.is_empty());
    assert!(outcome.installed_path.is_none());
    assert!(outcome.mods_config_backup.is_none());

    let export_path = outcome.export_path.clone();
    assert!(export_path.is_dir(), "the exported folder must exist");
    assert!(export_path.join("About/About.xml").is_file());
    assert!(export_path.join("rimmerge.json").is_file());
    assert!(
        export_path.join("Patches/rimmerge_ThingDef.xml").is_file(),
        "a resolved Merge decision must produce a patch file"
    );

    let about_bytes = fs::read(export_path.join("About/About.xml")).expect("read About.xml");
    let about =
        about_xml::parse(&about_bytes, GameVersion::new(1, 6)).expect("About.xml must parse");
    let mut declared_deps: Vec<ModId> = about
        .declared
        .dependencies
        .iter()
        .map(|dependency| dependency.id.clone())
        .collect();
    declared_deps.sort();
    let mut declared_load_after = about.declared.load_after.clone();
    declared_load_after.sort();
    let expected_scope = vec![mod_a.clone(), mod_b.clone()];
    assert_eq!(
        declared_deps, expected_scope,
        "modDependencies must be exactly the declared scope"
    );
    assert_eq!(
        declared_load_after, expected_scope,
        "loadAfter must be exactly the declared scope"
    );

    let marker_bytes = fs::read(export_path.join("rimmerge.json")).expect("read rimmerge.json");
    let marker =
        rimmerge_marker::parse(&marker_bytes).expect("rimmerge.json must parse as a marker");
    assert_eq!(marker.kind, GeneratedKind::Patch);
    assert_eq!(marker.patch_id.as_deref(), Some(patch_id.as_str()));
    let expected_scope_set: std::collections::BTreeSet<ModId> =
        expected_scope.iter().cloned().collect();
    assert_eq!(marker.scope, Some(expected_scope_set));

    let patches_xml =
        fs::read_to_string(export_path.join("Patches/rimmerge_ThingDef.xml")).expect("read patch");
    assert!(
        patches_xml.contains("MaxHitPoints"),
        "the resolved field must land in the patch file: {patches_xml}"
    );
    assert!(
        patches_xml.contains("MayRequire"),
        "every operation must be MayRequire-gated: {patches_xml}"
    );

    // -- Exporting again must be byte-identical ---------------------------

    let first_export_snapshot = snapshot_files(&export_path);
    let second_outcome = export_patch
        .execute(
            &mut session,
            &patch_id,
            ExportOptions {
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .expect("exporting the patch a second time must succeed");
    assert_eq!(second_outcome.export_path, export_path);
    let second_export_snapshot = snapshot_files(&export_path);
    assert_eq!(
        first_export_snapshot, second_export_snapshot,
        "exporting the same decisions twice must be byte-identical"
    );

    let prev_backup = paths
        .profile_dir
        .join("patches")
        .join(format!("{patch_id}.prev"));
    assert!(
        prev_backup.join("About/About.xml").is_file(),
        "the second export must back up the first generation under patches/<id>.prev"
    );

    // -- ForeignFolder refusal against the real writer ---------------------

    let foreign_dir = scratch.path().join("foreign");
    let foreign_mod_dir = foreign_dir.join("sample_abcompat");
    fs::create_dir_all(&foreign_mod_dir).expect("seed a pre-existing foreign folder");
    fs::write(
        foreign_mod_dir.join("rimmerge.json"),
        br#"{"kind":"patch","patchId":"deadbeefcafe","scope":["some.other"]}"#,
    )
    .expect("seed a foreign marker naming a different patch id");

    let foreign_result = export_patch.execute(
        &mut session,
        &patch_id,
        ExportOptions {
            out_dir: foreign_dir,
            install: false,
        },
    );
    assert!(
        matches!(foreign_result, Err(ExportPatchError::ForeignFolder { .. })),
        "a folder marked as another patch's own export must be refused: {foreign_result:?}"
    );
    assert!(
        !foreign_mod_dir.join("About").exists(),
        "the real writer must never touch a folder it refused as foreign"
    );

    // Baseline, captured before install, to prove installing (and the
    // re-scan below) changes nothing about the profile's own findings or
    // its suggested order beyond appending the newly installed patch.
    let findings_before_install = session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            limit: 200,
            ..FindingFilter::default()
        },
    );
    let suggested_before_install: Vec<ModId> = session.orders().suggested.as_slice().to_vec();

    // -- ExportPatch { install: true } ------------------------------------

    let install_outcome = export_patch
        .execute(
            &mut session,
            &patch_id,
            ExportOptions {
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
        paths.game_dir.join("Mods").join("sample_abcompat")
    );
    assert!(install_outcome.mods_config_backup.is_some());

    let rewritten_mods_config =
        fs::read_to_string(&paths.mods_config).expect("re-read scratch ModsConfig.xml");
    let patch_position = rewritten_mods_config
        .find("<li>sample.abcompat</li>")
        .unwrap_or_else(|| panic!("must list the patch id: {rewritten_mods_config}"));
    for other in ["<li>fixture.moda</li>", "<li>fixture.modb</li>"] {
        let other_position = rewritten_mods_config
            .find(other)
            .unwrap_or_else(|| panic!("must still list {other}: {rewritten_mods_config}"));
        assert!(
            other_position < patch_position,
            "the patch's package id must be appended last, after {other}: {rewritten_mods_config}"
        );
    }

    // -- Re-scan: the patch is now a real, active mod folder --------------

    let mut second_session = load_project(paths.clone());
    let patch_mod = second_session
        .report()
        .mods
        .iter()
        .find(|m| m.id == ModId::new("sample.abcompat"))
        .expect("the installed patch must appear in the re-scanned report");
    let generated = patch_mod
        .generated
        .as_ref()
        .expect("the patch folder must carry a rimmerge.json marker");
    assert_eq!(generated.kind, GeneratedKind::Patch);

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
            .all(|key| !key.to_string().contains("sample.abcompat")),
        "no finding may name the installed compat patch: {all_findings:?}"
    );
    assert_eq!(
        all_findings.total, findings_before_install.total,
        "installing and re-scanning the patch must not change how many findings the profile ledger reports"
    );

    let mut suggested_after_reload: Vec<ModId> =
        second_session.orders().suggested.as_slice().to_vec();
    assert_eq!(
        suggested_after_reload.pop(),
        Some(ModId::new("sample.abcompat")),
        "the installed patch must be the only new entry, appended last"
    );
    assert_eq!(
        suggested_after_reload, suggested_before_install,
        "every other mod's suggested order must be unchanged by installing the patch"
    );

    let def_override_page = second_session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            kinds: Some(vec![FindingKind::DefOverride]),
            limit: 10,
            ..FindingFilter::default()
        },
    );
    assert_eq!(
        def_override_page.total, 1,
        "the profile ledger's own Fixture_Wall DefOverride finding is unaffected by the patch"
    );

    assert!(
        second_session.patch(&patch_id).is_some(),
        "the patch project itself must survive a reload from disk"
    );

    assert_eq!(
        second_session.orders().suggested.as_slice().last(),
        Some(&ModId::new("sample.abcompat")),
        "the suggested order must place the installed patch after both scope members"
    );
}
