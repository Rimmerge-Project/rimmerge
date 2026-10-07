//! `rimmerge order export|import` end to end, against a tempdir copy of
//! `rim-analyzer`'s checked-in fixture game tree plus a Core written into the
//! copy: `ludeon.rimworld` and `sample.mod` active, `aaa.mod`/`zzz.mod` on
//! disk but inactive. Every id here is invented, and
//! nothing touches a real install, `ModsConfig.xml` or `ModLists` folder.
//! The running-game probe refusal is covered by `commands::order`'s own
//! `#[cfg(test)]` module (an integration test has no seam for a fake probe),
//! so every real write here passes `--force`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

mod common;
use common::copy_dir_recursive;

fn sample_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-analyzer")
        .join("tests")
        .join("fixtures")
        .join("sample_game")
}

const CRLF_MODS_CONFIG: &str = "<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.0</version>\r\n  <activeMods>\r\n    <li>ludeon.rimworld</li>\r\n    <li>sample.mod</li>\r\n  </activeMods>\r\n  <knownExpansions>\r\n  </knownExpansions>\r\n</ModsConfigData>\r\n";

struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

fn scratch() -> Scratch {
    let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let game_dir = dir.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy fixture game tree: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590")
        .unwrap_or_else(|error| panic!("write Version.txt: {error}"));
    let core_about = game_dir.join("Data").join("Core").join("About");
    fs::create_dir_all(&core_about).unwrap_or_else(|error| panic!("create Core: {error}"));
    fs::write(
        core_about.join("About.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModMetaData>\n  <packageId>Ludeon.RimWorld</packageId>\n</ModMetaData>\n",
    )
    .unwrap_or_else(|error| panic!("write Core About.xml: {error}"));
    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, CRLF_MODS_CONFIG)
        .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));
    Scratch {
        root: dir.path().to_path_buf(),
        workshop_dir: dir.path().join("workshop_does_not_exist"),
        profile_dir: dir.path().join("profile"),
        game_dir,
        mods_config,
        _dir: dir,
    }
}

impl Scratch {
    /// `rimmerge order <args...>` with this scratch install's paths.
    fn cmd(&self, args: &[&str]) -> common::CliCommand {
        let mut cmd = common::rimmerge();
        cmd.arg("order")
            .args(args)
            .arg("--game-dir")
            .arg(&self.game_dir)
            .arg("--workshop-dir")
            .arg(&self.workshop_dir)
            .arg("--mods-config")
            .arg(&self.mods_config)
            .arg("--profile-dir")
            .arg(&self.profile_dir);
        cmd
    }

    fn read_mods_config(&self) -> Vec<u8> {
        fs::read(&self.mods_config).unwrap_or_else(|error| panic!("read ModsConfig.xml: {error}"))
    }

    fn backup_count(&self) -> usize {
        fs::read_dir(&self.game_dir)
            .unwrap_or_else(|error| panic!("read game dir: {error}"))
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .count()
    }

    /// The `<activeMods>` ids of `ModsConfig.xml`, in file order.
    fn active_ids(&self) -> Vec<String> {
        let text = String::from_utf8(self.read_mods_config())
            .unwrap_or_else(|error| panic!("ModsConfig.xml was not utf8: {error}"));
        let active = text
            .split("<activeMods>")
            .nth(1)
            .and_then(|rest| rest.split("</activeMods>").next())
            .unwrap_or_else(|| panic!("no <activeMods> in:\n{text}"));
        active
            .split("<li>")
            .skip(1)
            .filter_map(|item| item.split("</li>").next())
            .map(str::to_string)
            .collect()
    }
}

fn stdout_of(mut cmd: common::CliCommand) -> String {
    let output = cmd
        .output()
        .unwrap_or_else(|error| panic!("run command: {error}"));
    assert!(
        output.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout was not utf8: {error}"))
}

fn stderr_of_failure(mut cmd: common::CliCommand) -> String {
    let output = cmd
        .output()
        .unwrap_or_else(|error| panic!("run command: {error}"));
    assert!(!output.status.success(), "command should have failed");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

// -- export ---------------------------------------------------------------

#[test]
fn export_to_stdout_prints_the_text_format_and_is_byte_identical_across_runs() {
    let scratch = scratch();

    let first = stdout_of(scratch.cmd(&["export"]));
    let second = stdout_of(scratch.cmd(&["export"]));

    assert!(first.contains("[sample.mod]"), "got:\n{first}");
    assert!(first.starts_with("# RimWorld 1.6"), "got:\n{first}");
    assert_eq!(first, second);
}

#[test]
fn export_out_writes_an_rml_and_refuses_to_overwrite_without_the_flag() {
    let scratch = scratch();
    let out = scratch.root.join("shared.rml");
    let out_arg = out.to_string_lossy().into_owned();

    let wrote = stdout_of(scratch.cmd(&["export", "--out", &out_arg]));
    let rml = fs::read_to_string(&out).unwrap_or_else(|error| panic!("read .rml: {error}"));
    assert!(wrote.contains("wrote 2 mods"), "got:\n{wrote}");
    assert!(rml.contains("<savedModList>") && rml.contains("sample.mod"));

    let refusal = stderr_of_failure(scratch.cmd(&["export", "--out", &out_arg]));
    assert!(refusal.contains("--overwrite"), "got:\n{refusal}");
    assert_eq!(
        fs::read_to_string(&out).unwrap_or_default(),
        rml,
        "a refused export must leave the file alone"
    );

    stdout_of(scratch.cmd(&["export", "--out", &out_arg, "--overwrite"]));
}

#[test]
fn export_never_touches_mods_config() {
    let scratch = scratch();
    let before = scratch.read_mods_config();
    let out = scratch.root.join("shared.rml");

    stdout_of(scratch.cmd(&["export", "--out", &out.to_string_lossy()]));

    assert_eq!(scratch.read_mods_config(), before);
    assert_eq!(scratch.backup_count(), 0);
}

#[test]
fn export_overwrite_without_out_is_a_usage_error() {
    let scratch = scratch();

    let message = stderr_of_failure(scratch.cmd(&["export", "--overwrite"]));

    assert!(message.contains("--out"), "got:\n{message}");
}

// -- import ---------------------------------------------------------------

const REORDER_LIST: &str = "1. Zzz [zzz.mod]\n2. Sample [sample.mod]\n3. Aaa [aaa.mod]\n";

#[test]
fn import_dry_run_prints_the_plan_and_leaves_the_file_byte_unchanged() {
    let scratch = scratch();
    let before = scratch.read_mods_config();
    let mut cmd = scratch.cmd(&["import", "-", "--dry-run"]);
    cmd.write_stdin(REORDER_LIST);

    let text = stdout_of(cmd);

    assert!(text.contains("2 activated"), "got:\n{text}");
    assert!(
        text.contains("core: not in the list; kept first"),
        "got:\n{text}"
    );
    assert!(text.contains("(dry run"), "got:\n{text}");
    assert_eq!(scratch.read_mods_config(), before);
    assert_eq!(scratch.backup_count(), 0);
}

#[test]
fn import_writes_the_planned_order_with_a_backup_and_prints_the_apply_hint() {
    let scratch = scratch();
    let mut cmd = scratch.cmd(&["import", "-", "--force"]);
    cmd.write_stdin(REORDER_LIST);

    let text = stdout_of(cmd);

    assert_eq!(
        scratch.active_ids(),
        ["ludeon.rimworld", "zzz.mod", "sample.mod", "aaa.mod"]
    );
    assert_eq!(scratch.backup_count(), 1);
    assert!(
        text.contains("rimmerge apply --dry-run --source current"),
        "got:\n{text}"
    );
}

#[test]
fn a_lossy_import_without_yes_is_refused_and_leaves_the_file_unchanged() {
    let scratch = scratch();
    let before = scratch.read_mods_config();
    let mut cmd = scratch.cmd(&["import", "-", "--force"]);
    cmd.write_stdin("1. Aaa [aaa.mod]\n");

    let message = stderr_of_failure(cmd);

    assert!(message.contains("--yes"), "got:\n{message}");
    assert_eq!(scratch.read_mods_config(), before);
    assert_eq!(scratch.backup_count(), 0);
}

#[test]
fn a_lossy_import_with_yes_deactivates_what_the_list_omits() {
    let scratch = scratch();
    let mut cmd = scratch.cmd(&["import", "-", "--force", "--yes"]);
    cmd.write_stdin("1. Aaa [aaa.mod]\n");

    let text = stdout_of(cmd);

    assert_eq!(scratch.active_ids(), ["ludeon.rimworld", "aaa.mod"]);
    assert!(text.contains("deactivate (1)"), "got:\n{text}");
}

#[test]
fn a_missing_mod_is_listed_with_its_workshop_link_and_needs_yes() {
    let scratch = scratch();
    let list = "1. Sample [sample.mod]\n2. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>\n";
    let mut dry = scratch.cmd(&["import", "-", "--dry-run"]);
    dry.write_stdin(list);
    let text = stdout_of(dry);
    assert!(text.contains("not installed (1)"), "got:\n{text}");
    assert!(
        text.contains("https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890"),
        "got:\n{text}"
    );

    let mut real = scratch.cmd(&["import", "-", "--force"]);
    real.write_stdin(list);
    let message = stderr_of_failure(real);
    assert!(message.contains("--yes"), "got:\n{message}");
}

#[test]
fn importing_your_own_export_is_an_empty_diff_and_writes_nothing() {
    let scratch = scratch();
    let out = scratch.root.join("mine.rml");
    let out_arg = out.to_string_lossy().into_owned();
    stdout_of(scratch.cmd(&["export", "--out", &out_arg]));
    let before = scratch.read_mods_config();

    let text = stdout_of(scratch.cmd(&["import", &out_arg, "--force"]));

    assert!(
        text.contains("0 activated, 0 deactivated, 0 moved"),
        "got:\n{text}"
    );
    assert!(text.contains("already has this order"), "got:\n{text}");
    assert_eq!(scratch.read_mods_config(), before);
    assert_eq!(scratch.backup_count(), 0);
}

#[test]
fn import_reads_an_rml_file_exported_from_another_order() {
    let scratch = scratch();
    let out = scratch.root.join("shared.rml");
    let out_arg = out.to_string_lossy().into_owned();
    let mut seed = scratch.cmd(&["import", "-", "--force"]);
    seed.write_stdin(REORDER_LIST);
    stdout_of(seed);
    stdout_of(scratch.cmd(&["export", "--out", &out_arg]));
    let mut restore = scratch.cmd(&["import", "-", "--force", "--yes"]);
    restore.write_stdin("1. Sample [sample.mod]\n");
    stdout_of(restore);
    assert_eq!(scratch.active_ids(), ["ludeon.rimworld", "sample.mod"]);

    stdout_of(scratch.cmd(&["import", &out_arg, "--force"]));

    assert_eq!(
        scratch.active_ids(),
        ["ludeon.rimworld", "zzz.mod", "sample.mod", "aaa.mod"]
    );
}

#[test]
fn an_oversized_stdin_is_rejected_and_nothing_is_written() {
    let scratch = scratch();
    let before = scratch.read_mods_config();
    let mut cmd = scratch.cmd(&["import", "-", "--force", "--yes"]);
    cmd.write_stdin(vec![b'a'; 4 * 1024 * 1024 + 1]);

    let message = stderr_of_failure(cmd);

    assert!(message.contains("larger than"), "got:\n{message}");
    assert_eq!(scratch.read_mods_config(), before);
}

#[test]
fn input_that_is_not_a_list_is_rejected_with_a_reason() {
    let scratch = scratch();
    let mut cmd = scratch.cmd(&["import", "-", "--dry-run"]);
    cmd.write_stdin("   \n# only a comment\n");

    let message = stderr_of_failure(cmd);

    assert!(message.contains("cannot import"), "got:\n{message}");
}

#[test]
fn a_missing_file_is_an_error() {
    let scratch = scratch();
    let absent = scratch.root.join("absent.rml");

    let message =
        stderr_of_failure(scratch.cmd(&["import", &absent.to_string_lossy(), "--dry-run"]));

    assert!(message.contains("absent.rml"), "got:\n{message}");
}

#[test]
fn a_list_that_names_core_keeps_its_position_and_prints_no_core_note() {
    let scratch = scratch();
    let mut cmd = scratch.cmd(&["import", "-", "--force"]);
    cmd.write_stdin("1. Sample [sample.mod]\n2. Core [ludeon.rimworld]\n");

    let text = stdout_of(cmd);

    assert_eq!(scratch.active_ids(), ["sample.mod", "ludeon.rimworld"]);
    assert!(!text.contains("core:"), "got:\n{text}");
}

#[test]
fn an_import_that_would_leave_no_core_is_refused_and_the_file_is_unchanged() {
    let scratch = scratch();
    fs::remove_dir_all(scratch.game_dir.join("Data").join("Core"))
        .unwrap_or_else(|error| panic!("remove Core: {error}"));
    let before = scratch.read_mods_config();
    let mut cmd = scratch.cmd(&["import", "-", "--force", "--yes"]);
    cmd.write_stdin("1. Aaa [aaa.mod]\n");

    let message = stderr_of_failure(cmd);

    assert!(message.contains("no Core"), "got:\n{message}");
    assert_eq!(scratch.read_mods_config(), before);
    assert_eq!(scratch.backup_count(), 0);
}

#[test]
fn an_rml_piped_on_stdin_is_read_like_the_file() {
    let scratch = scratch();
    let out = scratch.root.join("mine.rml");
    stdout_of(scratch.cmd(&["export", "--out", &out.to_string_lossy()]));
    let rml = fs::read(&out).unwrap_or_else(|error| panic!("read .rml: {error}"));
    let mut cmd = scratch.cmd(&["import", "-", "--dry-run"]);
    cmd.write_stdin(rml);

    let text = stdout_of(cmd);

    assert!(
        text.contains("0 activated, 0 deactivated, 0 moved"),
        "got:\n{text}"
    );
}

/// Discovery itself needs `Version.txt` (every `mods` command fails the
/// same way), so a missing one is reported by name, never papered over.
#[test]
fn a_missing_version_txt_is_reported_by_name() {
    let scratch = scratch();
    fs::remove_file(scratch.game_dir.join("Version.txt"))
        .unwrap_or_else(|error| panic!("remove Version.txt: {error}"));

    let export = stderr_of_failure(scratch.cmd(&["export"]));
    let mut import = scratch.cmd(&["import", "-", "--dry-run"]);
    import.write_stdin(REORDER_LIST);
    let import = stderr_of_failure(import);

    assert!(export.contains("Version.txt"), "got:\n{export}");
    assert!(import.contains("Version.txt"), "got:\n{import}");
}
