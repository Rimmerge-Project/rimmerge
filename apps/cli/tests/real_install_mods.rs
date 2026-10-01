//! Real-install round-trip for `rimmerge mods activate|deactivate`:
//! deactivates then re-activates one real, active, non-Core mod against a
//! **copy** of the real `ModsConfig.xml`, and cross-checks `mods list
//! --inactive`'s own count against a direct, ground-truth
//! `rim_analyzer::infra::inventory` call over the same paths.
//!
//! **Never touches the real `ModsConfig.xml`**: the real file is read
//! once (to copy it), every `mods` invocation below points `--mods-config`
//! at the copy under `RIMMERGE_PERF_PROFILE_DIR`.
//!
//! Round-trip is checked both ways: on the *parsed* active-id list (every
//! other id's relative order unchanged, the reactivated mod now last —
//! RimWorld's own append-at-the-end placement) and, separately, on
//! the raw **lines** of the file. The line check is a real, tight
//! invariant here, not a loose one — `ModsConfigFileStore::write_with_backup`
//! always re-renders in its own canonical two-space-indented shape
//! (`rim-io`'s own `CLAUDE.md`), and on the reference install that shape
//! already matches RimWorld's own real, game-written file byte for byte
//! (2-space `<version>`/`<activeMods>`, 4-space `<li>`, CRLF): every
//! line is unchanged except the target's own `<li>` line, moved to just
//! before `</activeMods>`.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo test -p rimmerge-cli --release --test real_install_mods -- --ignored --nocapture`

mod common;

use std::fs;

use rim_analyzer::domain::{FolderPolicy, ModId};
use rim_analyzer::extract::mods_config::parse_mods_config;
use rim_analyzer::infra;

const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo test -p rimmerge-cli \
     --release --test real_install_mods -- --ignored --nocapture";

/// The index of the line reading `</activeMods>` (whitespace trimmed).
fn active_mods_close_index(lines: &[&str]) -> usize {
    lines
        .iter()
        .position(|line| line.trim() == "</activeMods>")
        .unwrap_or_else(|| panic!("no </activeMods> line found in:\n{}", lines.join("\n")))
}

/// The index of the line naming `target`'s own `<li>` entry
/// (case-insensitive, whitespace trimmed — a real `ModsConfig.xml` is
/// not guaranteed to store every id lowercase even though `ModId` always
/// compares that way).
fn target_line_index(lines: &[&str], target: &ModId) -> usize {
    let needle = format!("<li>{target}</li>");
    lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case(&needle))
        .unwrap_or_else(|| panic!("no <li> line for {target} found in:\n{}", lines.join("\n")))
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch dir"]
fn deactivate_then_reactivate_round_trips_a_copy_of_the_real_mods_config() {
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let game_dir = common::require_game_dir(RERUN_COMMAND)
        .unwrap_or_else(|| panic!("require_profile_dir already confirmed the install is present"));
    let workshop_dir = common::real_workshop_dir(&game_dir);
    let real_mods_config = common::real_mods_config_path();
    common::assert_active_mod_floor(&real_mods_config, RERUN_COMMAND);

    fs::create_dir_all(&profile_dir)
        .unwrap_or_else(|error| panic!("create scratch profile dir: {error}"));
    let mods_config_copy = profile_dir.join("real_mods_config_copy.xml");
    fs::copy(&real_mods_config, &mods_config_copy).unwrap_or_else(|error| {
        panic!(
            "copy {} to {}: {error}",
            real_mods_config.display(),
            mods_config_copy.display()
        )
    });

    let original_bytes = fs::read(&mods_config_copy).expect("read the copy");
    let original = parse_mods_config(&original_bytes).expect("parse the copy");
    // Core (`ludeon.rimworld`) is **not** guaranteed to be first (a
    // real-install finding this same test surfaced — see
    // `crates/rim-session/CLAUDE.md`'s "Activating and deactivating mods"
    // note: an early-loading/runtime-patching-style mod legitimately loads
    // ahead of it), so filter it out explicitly rather than trusting a fixed
    // index. The middle of what's left, not the first or last entry, so
    // "moved to the end" is a real, observable change either way.
    let non_core: Vec<&ModId> = original
        .active_mods
        .iter()
        .filter(|id| id.base() != ModId::new("ludeon.rimworld"))
        .collect();
    assert!(
        !non_core.is_empty(),
        "expected at least one non-Core active mod on the real install"
    );
    let target = non_core[non_core.len() / 2].clone();

    let mut deactivate = common::rimmerge();
    deactivate
        .arg("mods")
        .arg("deactivate")
        .arg(target.as_str())
        // A real install's own dependency graph is out of this test's
        // control — `--yes` makes the deactivate succeed regardless of
        // whether some other active mod happens to declare `target` a
        // dependency.
        .arg("--yes")
        // `--mods-config` below points at `mods_config_copy`, a scratch
        // copy under `RIMMERGE_PERF_PROFILE_DIR` — never the real,
        // game-owned file — so bypassing the running-game probe here is
        // safe regardless of whether RimWorld happens to be running on
        // this machine.
        .arg("--force")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(&workshop_dir)
        .arg("--mods-config")
        .arg(&mods_config_copy)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .assert()
        .success();

    let mut activate = common::rimmerge();
    activate
        .arg("mods")
        .arg("activate")
        .arg(target.as_str())
        // Same as the deactivate call above: `mods_config_copy` is a
        // scratch copy, not the game's own file.
        .arg("--force")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(&workshop_dir)
        .arg("--mods-config")
        .arg(&mods_config_copy)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .assert()
        .success();

    let final_bytes = fs::read(&mods_config_copy).expect("read the copy after round-trip");
    let round_tripped = parse_mods_config(&final_bytes).expect("parse the round-tripped copy");

    assert_eq!(
        round_tripped.version, original.version,
        "<version> must round-trip unchanged"
    );
    assert_eq!(
        round_tripped.known_expansions, original.known_expansions,
        "<knownExpansions> must round-trip unchanged"
    );
    assert_eq!(
        round_tripped.active_mods.last(),
        Some(&target),
        "the reactivated mod must be appended at the end of the active list"
    );
    let original_without_target: Vec<ModId> = original
        .active_mods
        .iter()
        .filter(|id| **id != target)
        .cloned()
        .collect();
    let round_tripped_without_target: Vec<ModId> = round_tripped
        .active_mods
        .iter()
        .filter(|id| **id != target)
        .cloned()
        .collect();
    assert_eq!(
        round_tripped_without_target, original_without_target,
        "every other mod's relative order must be unchanged"
    );

    // The tight, line-level invariant this module's own doc comment
    // describes: every line unchanged except the target's own `<li>`
    // line, moved to just before `</activeMods>`.
    assert!(
        final_bytes.windows(2).any(|w| w == b"\r\n"),
        "CRLF line endings must be preserved"
    );
    let original_text = String::from_utf8(original_bytes.clone()).expect("original utf8");
    let final_text = String::from_utf8(final_bytes.clone()).expect("final utf8");
    let mut expected_lines: Vec<&str> = original_text.lines().collect();
    let target_index = target_line_index(&expected_lines, &target);
    let target_line = expected_lines.remove(target_index);
    let close_index = active_mods_close_index(&expected_lines);
    expected_lines.insert(close_index, target_line);
    let final_lines: Vec<&str> = final_text.lines().collect();
    assert_eq!(
        final_lines, expected_lines,
        "the round-tripped file must match the original line for line, except the \
         target's own <li> line moved to just before </activeMods>"
    );

    // Ground truth: a direct, discovery-only call against the exact same
    // paths `mods list` itself used.
    let game_version = infra::paths::default_game_version(&game_dir)
        .unwrap_or_else(|error| panic!("resolving game version: {error}"));
    let scan_config = infra::ScanConfig {
        game_dir: game_dir.clone(),
        workshop_dir: workshop_dir.clone(),
        mods_config_path: mods_config_copy.clone(),
        game_version,
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    };
    let ground_truth =
        infra::inventory(&scan_config).unwrap_or_else(|error| panic!("inventory: {error}"));
    // `discovered` (every mod found on disk) = every active id actually
    // found (`active.len() - missing.len()`) + `inactive.len()` — see
    // `ScanOutput::discovered_mod_count`'s own doc comment, which this
    // discovery-only path mirrors.
    let expected_inactive =
        ground_truth.discovered.len() - (ground_truth.active.len() - ground_truth.missing.len());

    let mut list = common::rimmerge();
    let output = list
        .arg("mods")
        .arg("list")
        .arg("--inactive")
        .arg("--json")
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--workshop-dir")
        .arg(&workshop_dir)
        .arg("--mods-config")
        .arg(&mods_config_copy)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .output()
        .expect("run mods list --inactive --json");
    assert!(output.status.success(), "mods list --inactive must succeed");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse mods list --json output");
    let cli_inactive_count = json["inactive"]
        .as_array()
        .expect("inactive must be a JSON array")
        .len();

    assert_eq!(
        cli_inactive_count, expected_inactive,
        "mods list --inactive's own count must equal discovered - active (missing excluded)"
    );

    eprintln!(
        "real-install mods round-trip: {} active, {} inactive, {} missing, {} discovered",
        ground_truth.active.len(),
        cli_inactive_count,
        ground_truth.missing.len(),
        ground_truth.discovered.len()
    );
}
