//! End-to-end: `LoadProject` -> `DecideMerge` -> `Apply { write_merge_mod:
//! true }`, wired to every real `rim-io` adapter, against a scratch copy of
//! this crate's own `merge_game` fixture (`ModA`/`ModB` both defining
//! `ThingDef/Fixture_Wall` through a two-level `ParentName` chain, with
//! differing `statBases/MaxHitPoints`, and both shipping a distinct
//! `Textures/Things/Fixture_Wall.png`) — never the real game install,
//! never the real `ModsConfig.xml`. Proves the whole merge pipeline
//! (scan -> plan -> decide -> render -> write -> re-scan) round-trips
//! together, and that `FileAssetLocator` locates and reads a real texture
//! back.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_io::{
    AnalyzerScanner, FileAssetLocator, FileDefSourceReader, JsonDecisionStore, JsonRuleStore,
    MergeModFolderWriter, ModsConfigFileStore,
};
use rim_resolve::domain::GeneratedModIdentity;
use rim_session::ports::TextureFormat;
use rim_session::use_cases::{Apply, ApplyOptions, DecideMerge, LoadProject, ReadTexture};
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
        rim_session::ports::NoPatchStore,
        rim_session::ports::NoAssignmentStore,
        rim_session::test_support::FakeModKnowledgeStore::empty(),
        true,
    );
    use_case
        .execute(paths, &mut |_progress| {})
        .unwrap_or_else(|error| panic!("loading the scratch project must succeed: {error}"))
}

#[test]
fn merge_decide_and_apply_writes_a_real_merge_mod_and_the_next_scan_sees_it() {
    let scratch = tempdir().expect("tempdir");
    let paths = scratch_paths(scratch.path()).expect("build scratch paths");

    let mut session = load_project(paths.clone());

    let def_override_page = session.findings(
        rim_resolve::domain::OrderSource::Current,
        &FindingFilter {
            kinds: Some(vec![FindingKind::DefOverride]),
            limit: 10,
            ..FindingFilter::default()
        },
    );
    assert_eq!(
        def_override_page.total, 1,
        "the fixture's ModA/ModB must produce exactly one DefOverride finding"
    );
    let key = def_override_page.items[0].clone();

    let decide_merge = DecideMerge::new(JsonDecisionStore::new(), FileDefSourceReader::new());
    let state = decide_merge
        .execute(&mut session, &key, std::collections::BTreeMap::new())
        .expect("deciding the merge must succeed");
    assert!(
        matches!(state, rim_resolve::domain::MergeState::Complete { .. }),
        "with no stored choices, MaxHitPoints is OneSided toward ModB (the winner) -> a no-op, complete plan: got {state:?}"
    );

    let apply = Apply::new(
        ModsConfigFileStore::new(),
        JsonDecisionStore::new(),
        JsonRuleStore::new(),
        MergeModFolderWriter::new(),
        FileDefSourceReader::new(),
        FileAssetLocator::new(),
    );
    let outcome = apply
        .execute(
            &mut session,
            ApplyOptions {
                source: rim_resolve::domain::OrderSource::Current,
                write_mods_config: true,
                write_merge_mod: true,
            },
        )
        .expect("applying with write_merge_mod must succeed");

    let merge_mod_path = outcome
        .merge_mod_path
        .expect("a Merge decision with a Complete state must produce a written merge mod");
    assert!(
        merge_mod_path.is_dir(),
        "the merge mod folder must exist on disk"
    );
    let about_text =
        fs::read_to_string(merge_mod_path.join("About/About.xml")).expect("read About.xml");
    assert!(
        about_text.contains("<packageId>rimmerge.merge."),
        "About.xml must carry the generated packageId: {about_text}"
    );
    roxmltree::Document::parse(&about_text).expect("About.xml must be well-formed XML");

    let identity = GeneratedModIdentity::for_profile(paths.profile_hash());
    let rewritten_mods_config =
        fs::read_to_string(&paths.mods_config).expect("re-read ModsConfig.xml");
    let merge_id_li = format!("<li>{}</li>", identity.package_id);
    let merge_id_position = rewritten_mods_config.find(&merge_id_li).unwrap_or_else(|| {
        panic!("ModsConfig.xml must list the merge mod id: {rewritten_mods_config}")
    });
    for other in ["<li>fixture.moda</li>", "<li>fixture.modb</li>"] {
        let other_position = rewritten_mods_config.find(other).unwrap_or_else(|| {
            panic!("ModsConfig.xml must still list {other}: {rewritten_mods_config}")
        });
        assert!(
            other_position < merge_id_position,
            "the merge mod id must be appended last, after {other}: {rewritten_mods_config}"
        );
    }

    // Re-scan from scratch: the generated mod is now a real mod folder on
    // disk, named in ModsConfig.xml.
    let mut second_session = load_project(paths.clone());

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
            .all(|key| !key.to_string().contains(&identity.package_id.to_string())),
        "no finding may name the generated merge mod: {all_findings:?}"
    );

    assert_eq!(
        second_session.orders().suggested.as_slice().last(),
        Some(&identity.package_id),
        "the suggested order must place the generated merge mod last"
    );

    // `ReadTexture`/`FileAssetLocator`: both mods ship a distinct
    // `Textures/Things/Fixture_Wall.png`, locatable by its normalized key.
    let read_texture = ReadTexture::new(FileAssetLocator::new());
    let mod_a_texture = read_texture
        .execute(&session, &ModId::new("fixture.moda"), "things/fixture_wall")
        .expect("ModA's Fixture_Wall texture must be locatable and readable");
    let mod_b_texture = read_texture
        .execute(&session, &ModId::new("fixture.modb"), "things/fixture_wall")
        .expect("ModB's Fixture_Wall texture must be locatable and readable");
    assert_eq!(mod_a_texture.texture.format, TextureFormat::Png);
    assert_eq!(mod_b_texture.texture.format, TextureFormat::Png);
    assert_ne!(
        mod_a_texture.texture.bytes, mod_b_texture.texture.bytes,
        "ModA and ModB must ship distinguishable textures at the same normalized path"
    );
}
