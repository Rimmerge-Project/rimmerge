//! The analyzer binary must resolve the config for its selected install.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    fs::write(path, contents).expect("fixture file");
}

fn analyze(home: &Path, game: &Path, explicit: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rim-analyzer"));
    command
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("USERPROFILE")
        .env_remove("LOCALAPPDATA")
        .args(["analyze", "--game-dir"])
        .arg(game);
    if let Some(config) = explicit {
        command.arg("--mods-config").arg(config);
    }
    command.output().expect("analyzer process")
}

#[test]
fn analyzer_uses_native_flatpak_and_secondary_proton_configs_and_preserves_override() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "rim-analyzer-install-config-{}",
        std::process::id()
    )));
    fs::create_dir_all(&scratch.0).expect("scratch");
    let suffix = "unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml";
    let xml = "<ModsConfigData><activeMods/></ModsConfigData>";
    for layout in ["native", "flatpak", "proton"] {
        let root = scratch.0.join(layout);
        let home = root.join("home");
        let steam = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        let library = root.join("secondary");
        let game = if layout == "native" {
            root.join("game")
        } else {
            library.join("steamapps/common/RimWorld")
        };
        fs::create_dir_all(game.join("Data/Core")).expect("core");
        let data = if layout == "proton" {
            "RimWorldWin64_Data"
        } else {
            "RimWorldLinux_Data"
        };
        fs::create_dir_all(game.join(data)).expect("runtime layout");
        write(&game.join("Version.txt"), "1.6.0");
        write(
            &steam.join("steamapps/libraryfolders.vdf"),
            &format!(
                "\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}",
                library.display()
            ),
        );
        write(
            &library.join("steamapps/appmanifest_294100.acf"),
            "\"AppState\"\n{\n\"appid\" \"294100\"\n\"installdir\" \"RimWorld\"\n}",
        );
        let config = match layout {
            "native" => home.join(".config").join(suffix),
            "flatpak" => home.join(".var/app/com.valvesoftware.Steam/.config").join(suffix),
            _ => library.join("steamapps/compatdata/294100/pfx/drive_c/users/steamuser/AppData/LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Config/ModsConfig.xml"),
        };
        if layout != "native" {
            write(
                &home.join(".config").join(suffix),
                "invalid native config decoy",
            );
        }
        write(&config, xml);
        let output = analyze(&home, &game, None);
        assert!(
            output.status.success(),
            "{layout}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_file(&config).expect("remove selected config");
        let output = analyze(&home, &game, None);
        assert!(
            !output.status.success(),
            "missing {layout} config must fail"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("select ModsConfig.xml explicitly")
        );
        let explicit = root.join("explicit.xml");
        write(&explicit, xml);
        let output = analyze(&home, &game, Some(&explicit));
        assert!(
            output.status.success(),
            "explicit {layout}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
