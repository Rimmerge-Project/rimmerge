//! Hermetic native Linux pipeline: discovered Unity config, actual backup/write
//! and compatibility-patch export. Every path belongs to a disposable fixture.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};

mod common;

struct NativeInstall {
    game: PathBuf,
    home: PathBuf,
    config_home: PathBuf,
    mods_config: PathBuf,
    workshop: PathBuf,
}

impl NativeInstall {
    fn command(&self, profile: &Path, args: &[&str]) -> common::CliCommand {
        let mut command = common::rimmerge();
        command
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env_remove("USERPROFILE")
            .env_remove("LOCALAPPDATA")
            .env_remove(rim_io::GAME_DIR_VAR)
            .env_remove(rim_io::MODS_CONFIG_VAR)
            .env_remove(rim_io::WORKSHOP_DIR_VAR);
        // The config is deliberately omitted: resolve it from this native install.
        command
            .args(args)
            .arg("--game-dir")
            .arg(&self.game)
            .arg("--workshop-dir")
            .arg(&self.workshop)
            .arg("--profile-dir")
            .arg(profile);
        command
    }
}

fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("reading {dir:?}: {e}")) {
        let path = entry
            .unwrap_or_else(|e| panic!("directory entry: {e}"))
            .path();
        let name = path
            .strip_prefix(dir)
            .unwrap_or_else(|e| panic!("relative path: {e}"));
        if path.is_dir() {
            for (relative, bytes) in snapshot(&path) {
                files.push((name.join(relative), bytes));
            }
        } else {
            files.push((
                name.to_path_buf(),
                fs::read(&path).unwrap_or_else(|e| panic!("read: {e}")),
            ));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn native_discovery_scan_sort_verify_apply_and_patch_export_are_hermetic() {
    let root = tempfile::tempdir().expect("scratch root");
    let game = root.path().join("NativeGame");
    common::copy_dir_recursive(&common::merge_game_fixture(), &game).expect("copy fixture");
    fs::create_dir_all(game.join("Data/Core")).expect("valid install marker");
    let managed = game.join("RimWorldLinux_Data/Managed");
    fs::create_dir_all(&managed).expect("native Unity data");
    assert_eq!(
        rim_analyzer::infra::paths::managed_assemblies_dir(&game),
        managed
    );
    assert!(!game.join("RimWorldWin64_Data").exists());

    let config_home = root.path().join("ConfigHome");
    let config_dir = config_home.join("unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config");
    fs::create_dir_all(&config_dir).expect("native Unity config");
    let mods_config = config_dir.join("ModsConfig.xml");
    fs::rename(game.join("ModsConfig.xml"), &mods_config).expect("move config to Unity layout");
    let original_config = fs::read(&mods_config).expect("original config");
    let case_decoy = config_dir.join("modsconfig.xml");
    fs::write(&case_decoy, b"case-sensitive sentinel").expect("case decoy");
    fs::write(config_dir.join("Prefs.xml"), b"preferences sentinel").expect("unrelated config");
    let original_game = snapshot(&game);
    let install = NativeInstall {
        game,
        home: root.path().join("Home"),
        config_home,
        mods_config,
        workshop: root.path().join("Workshop"),
    };
    fs::create_dir_all(&install.home).expect("isolated HOME");
    let profile1 = root.path().join("profile1");
    let profile2 = root.path().join("profile2");
    let mut outputs = Vec::new();
    let mut reports = Vec::new();
    for profile in [&profile1, &profile2] {
        install.command(profile, &["load"]).assert().success();
        let report_path = profile.join("report.json");
        let mut report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).expect("scan report"))
                .expect("report JSON");
        assert_eq!(report["mods"].as_array().expect("mods").len(), 2);
        report["metadata"]["generated_at"] = serde_json::Value::Null;
        reports.push(report);
        let sort = common::rimmerge()
            .args(["sort", "--report"])
            .arg(&report_path)
            .assert()
            .success();
        let ledger = common::rimmerge()
            .args(["ledger", "--report"])
            .arg(&report_path)
            .assert()
            .success();
        assert!(String::from_utf8_lossy(&ledger.get_output().stdout).contains("Fixture_Wall"));
        let verify = install
            .command(profile, &["verify", "--source", "suggested", "--json"])
            .assert()
            .success();
        let _: serde_json::Value =
            serde_json::from_slice(&verify.get_output().stdout).expect("verify JSON");
        outputs.push((
            sort.get_output().stdout.clone(),
            ledger.get_output().stdout.clone(),
            verify.get_output().stdout.clone(),
        ));
    }
    assert_eq!(
        reports[0], reports[1],
        "native scan is deterministic apart from its timestamp"
    );
    assert_eq!(
        outputs[0], outputs[1],
        "sort, ledger and verify are deterministic"
    );

    let created = install
        .command(
            &profile1,
            &[
                "patch",
                "new",
                "--name",
                "Native compat",
                "--package-id",
                "mypatch.nativecompat",
                "--display-name",
                "Native Compatibility",
                "--scope",
                "fixture.moda,fixture.modb",
            ],
        )
        .assert()
        .success();
    let stdout = String::from_utf8(created.get_output().stdout.clone()).expect("patch id output");
    let patch_id = stdout
        .trim()
        .strip_prefix("created patch ")
        .expect("created patch id");
    install
        .command(
            &profile1,
            &[
                "patch",
                "decide",
                "--patch",
                patch_id,
                "--key",
                "def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]",
                "--choice",
                "statBases/MaxHitPoints=from:fixture.moda",
            ],
        )
        .assert()
        .success()
        .stdout(predicates::str::contains("Complete"));
    let export1 = root.path().join("export1");
    let export2 = root.path().join("export2");
    for out in [&export1, &export2] {
        install
            .command(&profile1, &["patch", "export", "--patch", patch_id])
            .arg("--out")
            .arg(out)
            .assert()
            .success();
        let patch =
            fs::read_to_string(out.join("mypatch_nativecompat/Patches/rimmerge_ThingDef.xml"))
                .expect("rendered patch");
        assert!(patch.contains("MaxHitPoints"));
        let about = fs::read_to_string(out.join("mypatch_nativecompat/About/About.xml"))
            .expect("About.xml");
        assert!(about.contains("mypatch.nativecompat"));
    }
    assert_eq!(
        snapshot(&export1),
        snapshot(&export2),
        "patch exports are byte-identical"
    );
    assert_eq!(
        fs::read(&install.mods_config).expect("config before apply"),
        original_config
    );

    // Bypass only the host process probe; writes remain restricted to the scratch fixture.
    let applied = install
        .command(&profile1, &["apply", "--force"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&applied.get_output().stdout);
    assert!(stdout.contains("wrote ModsConfig.xml: true"));
    let backup = stdout
        .lines()
        .find_map(|line| line.strip_prefix("backup: "))
        .expect("backup path");
    assert!(Path::new(backup).starts_with(&config_dir));
    assert_eq!(fs::read(backup).expect("backup bytes"), original_config);
    let written = fs::read(&install.mods_config).expect("written config");
    let parsed =
        rim_analyzer::extract::mods_config::parse_mods_config(&written).expect("valid XML");
    assert_eq!(
        parsed
            .active_mods
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["fixture.moda", "fixture.modb"],
        "apply writes the suggested order"
    );
    assert_eq!(
        fs::read(&case_decoy).expect("case decoy"),
        b"case-sensitive sentinel"
    );
    assert_eq!(
        fs::read(config_dir.join("Prefs.xml")).expect("preferences"),
        b"preferences sentinel"
    );
    assert_eq!(
        snapshot(&install.game),
        original_game,
        "apply/export preserve the install tree"
    );
    fs::copy(backup, &install.mods_config).expect("restore only the disposable config");
    assert_eq!(
        fs::read(&install.mods_config).expect("restored config"),
        original_config
    );
    // A wrong-case sibling must not become an implicit fallback or a write target.
    fs::rename(&install.mods_config, config_dir.join("saved-config.xml")).expect("hide config");
    let missing_profile = root.path().join("missing-profile");
    let before_failure = snapshot(root.path());
    install
        .command(&missing_profile, &["load"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("ModsConfig.xml"));
    assert!(!missing_profile.join("report.json").exists());
    assert_eq!(
        snapshot(root.path()),
        before_failure,
        "failed discovery writes nothing"
    );
}
