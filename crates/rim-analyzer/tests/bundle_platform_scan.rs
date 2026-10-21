//! Bundle selection follows the game install, including Windows/Proton on Linux.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::{FolderPolicy, GameVersion};
use rim_analyzer::{analysis, infra};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("fixture directory");
    fs::write(path, content).expect("fixture file");
}

fn bundle(folder: &Path, name: &str, texture: &str) {
    let path = folder.join("AssetBundles").join(name);
    write(&path, "bundle");
    write(
        &path.with_extension("manifest"),
        &format!("Assets:\n- Assets/Data/test.bundle/Textures/{texture}.png\nDependencies: {{}}\n"),
    );
}

#[test]
fn bundle_platform_scan_selects_runtime_assets_and_preserves_folder_priority() {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("rimmerge-bundle-platform-{}", std::process::id())),
    );
    for (data_dir, os_texture) in [
        ("RimWorldLinux_Data", "linux"),
        ("RimWorldWin64_Data", "windows"),
    ] {
        let game_dir = scratch.0.join(data_dir);
        fs::create_dir_all(game_dir.join(data_dir)).expect("runtime data");
        write(&game_dir.join("Version.txt"), "1.6.0");
        fs::create_dir_all(game_dir.join("Data/Core")).expect("core directory");
        let mod_dir = game_dir.join("Mods/BundleMod");
        write(
            &mod_dir.join("About/About.xml"),
            "<ModMetaData><name>Bundle Mod</name><packageId>test.bundle</packageId></ModMetaData>",
        );
        for (name, texture) in [
            ("shared", "shared"),
            ("things_linux", "linux"),
            ("things_win", "windows"),
            ("things_mac", "mac"),
        ] {
            // The version folder wins for eligible bundles; the root manifest
            // must not contribute even when it names a different texture.
            bundle(&mod_dir.join("1.6"), name, texture);
            bundle(&mod_dir, name, &format!("shadowed_{texture}"));
        }
        let mods_config_path = game_dir.join("ModsConfig.xml");
        write(
            &mods_config_path,
            "<ModsConfigData><activeMods><li>test.bundle</li></activeMods></ModsConfigData>",
        );
        let output = infra::scan(&infra::ScanConfig {
            workshop_dir: game_dir.join("workshop"),
            game_dir,
            mods_config_path,
            game_version: GameVersion::new(1, 6),
            folder_policy: FolderPolicy::LoadFolders,
            active_mods: None,
        })
        .expect("synthetic install scan");
        assert!(output.missing_mods.is_empty());
        assert_eq!(output.scanned_mods.len(), 1);
        assert_eq!(
            output.scanned_mods[0].bundle_textures,
            BTreeSet::from(["shared".to_string(), os_texture.to_string()]),
            "bundle textures for {data_dir}"
        );
        let index = analysis::source_index::build(&output);
        assert!(index.textures.is_non_loose("shared"));
        assert_eq!(index.textures.is_non_loose("linux"), os_texture == "linux");
        assert_eq!(
            index.textures.is_non_loose("windows"),
            os_texture == "windows"
        );
        assert!(!index.textures.is_non_loose("mac"));
        for texture in ["shared", "linux", "windows", "mac"] {
            assert!(!index.textures.is_non_loose(&format!("shadowed_{texture}")));
        }
    }
}
