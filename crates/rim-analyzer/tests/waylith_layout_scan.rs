//! End-to-end scan of scratch installs in the Waylith layout (RimWorld
//! 1.7: `RimWorld.exe`, every DLL at the root, `Assets.zip` per art
//! source) and its Unity counterpart. Everything is built in a temp dir;
//! no real install is read.

use std::fs;
use std::path::Path;

use rim_analyzer::domain::{FolderPolicy, GameLayout, GameVersion, ModId, ScanOutput};
use rim_analyzer::infra::{self, ScanConfig, paths};

/// A stored ZIP archive holding `names` with empty bodies.
fn zip_bytes(names: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut directory = Vec::new();
    for name in names {
        let local_offset = u32::try_from(bytes.len()).expect("small archive");
        bytes.extend_from_slice(b"PK\x03\x04");
        bytes.extend_from_slice(&[0; 22]);
        bytes.extend_from_slice(&u16::try_from(name.len()).expect("short").to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());

        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&[0; 24]);
        directory.extend_from_slice(&u16::try_from(name.len()).expect("short").to_le_bytes());
        directory.extend_from_slice(&[0; 12]);
        directory.extend_from_slice(&local_offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let directory_offset = u32::try_from(bytes.len()).expect("small archive");
    let count = u16::try_from(names.len()).expect("few entries");
    bytes.extend_from_slice(&directory);
    bytes.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    let directory_size = u32::try_from(directory.len()).expect("small archive");
    bytes.extend_from_slice(&directory_size.to_le_bytes());
    bytes.extend_from_slice(&directory_offset.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}

const RUNTIME_CONFIG: &str = r#"{"runtimeOptions":{"configProperties":{
"WaylithClr.AssemblyNameRedirects":"UnityEngine*=UnityStubs;Unity.Newtonsoft.Json=;Unity.*=UnityStubs;0PrepatcherAPI=Compat"}}}"#;

fn write_mod(dir: &Path, package_id: &str) {
    fs::create_dir_all(dir.join("About")).expect("about dir");
    fs::write(
        dir.join("About").join("About.xml"),
        format!("<ModMetaData><packageId>{package_id}</packageId></ModMetaData>"),
    )
    .expect("about");
}

/// A scratch install with Core and Royalty, an `Assets.zip` for each, and
/// the Waylith root files. `unity_player` adds the Unity player's own
/// executable and data folder, which turns it into a Unity install.
fn install(core_zip: Option<&[u8]>, unity_player: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let game = dir.path();
    fs::write(game.join("Version.txt"), "1.7.5014 rev1195").expect("version");
    write_mod(&game.join("Data").join("Core"), "ludeon.rimworld");
    write_mod(
        &game.join("Data").join("Royalty"),
        "ludeon.rimworld.royalty",
    );
    fs::write(game.join("RimWorld.exe"), b"MZ").expect("exe");
    fs::write(game.join("Assembly-CSharp.dll"), b"not a real dll").expect("dll");
    fs::write(game.join("Brutal.Core.Common.dll"), b"x").expect("dll");
    fs::write(game.join("RimWorld.runtimeconfig.json"), RUNTIME_CONFIG).expect("config");
    if let Some(bytes) = core_zip {
        fs::write(game.join("Assets.zip"), bytes).expect("core zip");
    }
    fs::write(
        game.join("Data").join("Royalty").join("Assets.zip"),
        zip_bytes(&[
            "Textures/Things/Drape_east.psd.dds.split",
            "Sounds/Boom.ogg",
        ]),
    )
    .expect("dlc zip");
    if unity_player {
        fs::write(game.join("RimWorldWin64.exe"), b"MZ").expect("unity exe");
        fs::create_dir(game.join("RimWorldWin64_Data")).expect("unity data");
    }
    fs::write(
        game.join("ModsConfig.xml"),
        "<ModsConfigData><activeMods><li>ludeon.rimworld</li><li>ludeon.rimworld.royalty</li></activeMods></ModsConfigData>",
    )
    .expect("mods config");
    dir
}

fn scan(game: &Path) -> ScanOutput {
    infra::scan(&ScanConfig {
        mods_config_path: game.join("ModsConfig.xml"),
        workshop_dir: game.join("workshop_does_not_exist"),
        game_dir: game.to_path_buf(),
        game_version: GameVersion::new(1, 7),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    })
    .expect("scan succeeds")
}

fn core_zip() -> Vec<u8> {
    zip_bytes(&[
        "Resources/Textures/Damage/Scratch1.psd.dds.split",
        "Resources/Textures/UI/Wall.png.dds",
        "Resources/Fonts/Arial.ttf",
    ])
}

#[test]
fn waylith_install_reads_assemblies_redirects_and_both_texture_sources() {
    let dir = install(Some(&core_zip()), false);
    assert_eq!(paths::detect_game_layout(dir.path()), GameLayout::Waylith);

    let output = scan(dir.path());

    for name in ["assembly-csharp", "brutal.core.common", "0prepatcherapi"] {
        assert!(
            output.vanilla_assembly_names.contains(name),
            "{name} should be vanilla"
        );
    }
    assert!(
        output
            .engine_assembly_prefixes
            .matches("unityengine.coremodule")
    );
    assert!(output.engine_assembly_prefixes.matches("unity.burst"));
    assert!(
        !output
            .engine_assembly_prefixes
            .matches("unity.newtonsoft.json"),
        "an empty-target redirect is an exclusion"
    );
    assert_eq!(
        output.core_resource_textures.iter().collect::<Vec<_>>(),
        ["damage/scratch1", "ui/wall"]
    );
    let royalty = output
        .scanned_mods
        .iter()
        .find(|scanned| scanned.info.id == ModId::new("ludeon.rimworld.royalty"))
        .expect("royalty scanned");
    assert_eq!(
        royalty.bundle_textures.iter().collect::<Vec<_>>(),
        ["things/drape_east"]
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.message.contains("cannot parse Assembly-CSharp.dll")),
        "the junk Assembly-CSharp.dll is a warning, not a failure"
    );
}

#[test]
fn a_corrupt_core_archive_is_a_warning_and_leaves_the_texture_set_empty() {
    let dir = install(Some(&[0x41; 200]), false);

    let output = scan(dir.path());

    assert!(output.core_resource_textures.is_empty());
    assert!(
        output.warnings.iter().any(
            |w| w.message.contains("Assets.zip") && w.message.contains("Core's resource index")
        )
    );
}

#[test]
fn a_missing_runtime_config_is_a_warning_and_no_redirects() {
    let dir = install(Some(&core_zip()), false);
    fs::remove_file(dir.path().join("RimWorld.runtimeconfig.json")).expect("remove");

    let output = scan(dir.path());

    assert!(!output.engine_assembly_prefixes.matches("unity.burst"));
    assert!(!output.vanilla_assembly_names.contains("0prepatcherapi"));
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.message.contains("assembly redirects"))
    );
}

#[test]
fn the_unity_executable_keeps_the_unity_behaviour_and_ignores_the_archives() {
    let dir = install(Some(&core_zip()), true);

    let output = scan(dir.path());

    assert!(output.core_resource_textures.is_empty());
    assert!(output.vanilla_assembly_names.is_empty());
    assert!(!output.engine_assembly_prefixes.matches("unity.burst"));
    let royalty = output
        .scanned_mods
        .iter()
        .find(|scanned| scanned.info.id == ModId::new("ludeon.rimworld.royalty"))
        .expect("royalty scanned");
    assert!(royalty.bundle_textures.is_empty());
}

/// Adds an active local (non-vanilla) mod `local.art` whose folder ships an
/// `Assets.zip`.
fn add_local_mod_with_archive(game: &Path) {
    write_mod(&game.join("Mods").join("LocalArt"), "local.art");
    fs::write(
        game.join("Mods").join("LocalArt").join("Assets.zip"),
        zip_bytes(&["Textures/Local/Thing.png.dds"]),
    )
    .expect("local zip");
    fs::write(
        game.join("ModsConfig.xml"),
        "<ModsConfigData><activeMods><li>ludeon.rimworld</li><li>ludeon.rimworld.royalty</li><li>local.art</li></activeMods></ModsConfigData>",
    )
    .expect("mods config");
}

#[test]
fn a_local_mods_assets_zip_contributes_no_archive_textures() {
    let dir = install(Some(&core_zip()), false);
    add_local_mod_with_archive(dir.path());

    let output = scan(dir.path());

    let local = output
        .scanned_mods
        .iter()
        .find(|scanned| scanned.info.id == ModId::new("local.art"))
        .expect("local mod scanned");
    assert!(local.bundle_textures.is_empty());
    let royalty = output
        .scanned_mods
        .iter()
        .find(|scanned| scanned.info.id == ModId::new("ludeon.rimworld.royalty"))
        .expect("royalty scanned");
    assert!(!royalty.bundle_textures.is_empty());
}

#[test]
fn a_corrupt_dlc_archive_warns_with_that_dlcs_id() {
    let dir = install(Some(&core_zip()), false);
    fs::write(
        dir.path().join("Data").join("Royalty").join("Assets.zip"),
        [0x41; 200],
    )
    .expect("corrupt zip");

    let output = scan(dir.path());

    let royalty_id = ModId::new("ludeon.rimworld.royalty");
    let warning = output
        .warnings
        .iter()
        .find(|w| w.message.contains("the DLC's textures"))
        .expect("a DLC archive warning");
    assert_eq!(warning.mod_id.as_ref(), Some(&royalty_id));
    let royalty = output
        .scanned_mods
        .iter()
        .find(|scanned| scanned.info.id == royalty_id)
        .expect("royalty scanned");
    assert!(royalty.bundle_textures.is_empty());
}
