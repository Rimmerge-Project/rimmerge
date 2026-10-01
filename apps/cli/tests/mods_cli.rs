//! `rimmerge mods list|activate|deactivate` end to end, against a tempdir
//! copy of
//! `rim-analyzer`'s checked-in fixture game tree — never the real game
//! install or the real `ModsConfig.xml`. The game-running probe refusal
//! is covered by `commands::mods`'s own `#[cfg(test)]` module (the same
//! `FixedProbe` shape `commands::apply`'s own tests use), not here — this
//! file only exercises the compiled binary end to end.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::PredicateBooleanExt;
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

/// A CRLF-terminated `ModsConfig.xml` naming the fixture's one mod
/// (`sample.mod`) — RimWorld's own line-ending convention, mirroring
/// `apply_dry_run.rs`'s own constant.
const CRLF_MODS_CONFIG: &str = "<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.0</version>\r\n  <activeMods>\r\n    <li>sample.mod</li>\r\n  </activeMods>\r\n  <knownExpansions>\r\n  </knownExpansions>\r\n</ModsConfigData>\r\n";

struct Scratch {
    _dir: tempfile::TempDir,
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

/// A fresh tempdir copy of `sample_game`, seeded with `Version.txt` and
/// the CRLF `ModsConfig.xml` above — `sample.mod` active, `aaa.mod`/
/// `zzz.mod` on disk but inactive.
///
/// Not a `#[test]` function itself, so `clippy::expect_used`'s test
/// allowance doesn't apply here (`common::copy_dir_recursive`'s own doc
/// comment names the same rule) — every fallible step panics via
/// `unwrap_or_else` instead.
fn scratch() -> Scratch {
    let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let game_dir = dir.path().join("game");
    copy_dir_recursive(&sample_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy fixture game tree: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590")
        .unwrap_or_else(|error| panic!("write Version.txt: {error}"));
    let mods_config = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config, CRLF_MODS_CONFIG)
        .unwrap_or_else(|error| panic!("write CRLF ModsConfig.xml: {error}"));

    Scratch {
        workshop_dir: dir.path().join("workshop_does_not_exist"),
        profile_dir: dir.path().join("profile"),
        game_dir,
        mods_config,
        _dir: dir,
    }
}

impl Scratch {
    fn cmd(&self, args: &[&str]) -> common::CliCommand {
        let mut cmd = common::rimmerge();
        cmd.arg("mods")
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

    fn backup_files(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.game_dir)
            .unwrap_or_else(|error| panic!("read game dir: {error}"))
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .map(|entry| entry.path())
            .collect()
    }
}

/// Appends a `<modDependencies>` entry naming `dependency` to `mod_dir`'s
/// own `About/About.xml` (already-parsed content, rewritten whole — the
/// fixture's own `AaaMod`/`SampleMod` About.xml files are tiny and this
/// crate's other tests never pin their exact text). Both ids used by this
/// file's own tests are neutral, fixture-only ids (`aaa.mod`, `zzz.mod`,
/// `sample.mod`), never a real mod name.
fn add_mod_dependency(mod_dir: &Path, package_id: &str, name: &str, dependency: &str) {
    let about_path = mod_dir.join("About").join("About.xml");
    let about = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModMetaData>\n  <packageId>{package_id}</packageId>\n  <name>{name}</name>\n  <author>Fixture Author</author>\n  <supportedVersions>\n    <li>1.6</li>\n  </supportedVersions>\n  <modDependencies>\n    <li>\n      <packageId>{dependency}</packageId>\n      <displayName>{dependency}</displayName>\n    </li>\n  </modDependencies>\n</ModMetaData>\n"
    );
    fs::write(&about_path, about)
        .unwrap_or_else(|error| panic!("rewrite {}: {error}", about_path.display()));
}

/// Creates a Workshop-*only* mod folder (no local `Mods` copy at all)
/// under the scratch tree's own workshop dir — the exact shape the
/// `_steam`-suffixed Workshop-only test needs.
fn add_workshop_only_mod(scratch: &Scratch, workshop_id: &str, package_id: &str, name: &str) {
    let about_dir = scratch.workshop_dir.join(workshop_id).join("About");
    fs::create_dir_all(&about_dir).unwrap_or_else(|error| panic!("create {about_dir:?}: {error}"));
    fs::write(about_dir.join("About.xml"),
        format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModMetaData>\n  <packageId>{package_id}</packageId>\n  <name>{name}</name>\n  <author>Fixture Author</author>\n</ModMetaData>\n"
        ))
    .unwrap_or_else(|error| panic!("write About.xml for {package_id}: {error}"));
}

/// Overwrites the scratch tree's `ModsConfig.xml` to activate exactly
/// `active`, verbatim, in order — CRLF, matching RimWorld's own
/// convention.
fn set_active_mods(scratch: &Scratch, active: &[&str]) {
    let lis: String = active
        .iter()
        .map(|id| format!("    <li>{id}</li>\r\n"))
        .collect();
    fs::write(&scratch.mods_config,
        format!("<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.0</version>\r\n  <activeMods>\r\n{lis}  </activeMods>\r\n</ModsConfigData>\r\n"
        ))
    .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));
}

/// Runs `cmd` to completion and returns `(succeeded, stdout as text)` —
/// for a test that needs to check *where* a row appears in `mods list
/// --all`'s output, not merely that it appears somewhere
/// (`predicates::str::contains` alone can't distinguish the active
/// section from the inactive one).
fn run_captured(mut cmd: common::CliCommand) -> (bool, String) {
    let output = cmd
        .output()
        .unwrap_or_else(|error| panic!("run command: {error}"));
    (
        output.status.success(),
        String::from_utf8(output.stdout)
            .unwrap_or_else(|error| panic!("stdout was not utf8: {error}")),
    )
}

/// The text from `start`'s own first occurrence (inclusive) up to the
/// next occurrence of `end` (exclusive, or end-of-string when `end`
/// never appears) — `mods list --all`'s own section boundaries
/// (`"active ("`, `"inactive ("`, `"missing ("`), so a row's presence
/// can be checked against the *right* section instead of the whole
/// output.
fn section_text<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = text
        .find(start)
        .unwrap_or_else(|| panic!("{start:?} not found in:\n{text}"));
    let after_start = &text[start_idx..];
    let end_idx = after_start.find(end).unwrap_or(after_start.len());
    &after_start[..end_idx]
}

// -- list ---------------------------------------------------------------

#[test]
fn list_inactive_shows_the_two_discovered_but_inactive_fixture_mods() {
    let scratch = scratch();

    scratch
        .cmd(&["list", "--inactive"])
        .assert()
        .success()
        .stdout(predicates::str::contains("aaa.mod"))
        .stdout(predicates::str::contains("zzz.mod"))
        .stdout(predicates::str::contains("sample.mod").not());
}

/// Asserts *section membership*, not merely that each id appears
/// somewhere in the output — a bug that moved a row into the wrong
/// section, or duplicated it across two, would still satisfy a bare
/// `contains` check.
#[test]
fn list_all_shows_active_inactive_and_missing_sections() {
    let scratch = scratch();

    let (success, text) = run_captured(scratch.cmd(&["list", "--all"]));
    assert!(success, "mods list --all must succeed:\n{text}");

    let active_section = section_text(&text, "active (", "inactive (");
    let inactive_section = section_text(&text, "inactive (", "missing (");
    let missing_section = &text[text
        .find("missing (")
        .expect("a missing section header must be present")..];

    assert!(
        active_section.contains("sample.mod"),
        "sample.mod must be listed in the active section:\n{active_section}"
    );
    assert!(!active_section.contains("aaa.mod"));
    assert!(!active_section.contains("zzz.mod"));

    assert!(
        inactive_section.contains("aaa.mod"),
        "aaa.mod must be listed in the inactive section:\n{inactive_section}"
    );
    assert!(
        inactive_section.contains("zzz.mod"),
        "zzz.mod must be listed in the inactive section:\n{inactive_section}"
    );
    assert!(!inactive_section.contains("sample.mod"));

    assert!(
        missing_section.contains("missing (0)"),
        "the fixture has no missing mods:\n{missing_section}"
    );
}

/// A Workshop-only mod (no local `Mods` copy at all) active under its
/// `_steam`-suffixed id must be recognized by both `activate`/
/// `deactivate` and must appear exactly once — in the active section —
/// from `mods list --all`; never under the bare id, and never a second
/// time as inactive or missing.
#[test]
fn a_steam_suffixed_workshop_only_mod_is_recognized_and_listed_exactly_once() {
    let scratch = scratch();
    add_workshop_only_mod(&scratch, "444444", "x.mod", "X Mod");
    set_active_mods(&scratch, &["sample.mod", "x.mod_steam"]);

    // Both of these must accept "x.mod_steam" as a known mod — the
    // inventory keys the folder under the exact active id, not the bare
    // `x.mod`.
    scratch
        .cmd(&["deactivate", "x.mod_steam", "--dry-run"])
        .assert()
        .success();
    scratch
        .cmd(&["activate", "x.mod_steam", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already active"));

    let (success, text) = run_captured(scratch.cmd(&["list", "--all"]));
    assert!(success, "mods list --all must succeed:\n{text}");

    let active_section = section_text(&text, "active (", "inactive (");
    let inactive_section = section_text(&text, "inactive (", "missing (");
    let missing_section = &text[text
        .find("missing (")
        .expect("a missing section header must be present")..];

    assert!(
        active_section.contains("x.mod_steam"),
        "x.mod_steam must be listed in the active section:\n{active_section}"
    );
    assert!(
        !inactive_section.contains("x.mod"),
        "the same folder must not also appear inactive:\n{inactive_section}"
    );
    assert!(
        !missing_section.contains("x.mod"),
        "the same folder must not also appear missing:\n{missing_section}"
    );
}

// -- activate -------------------------------------------------------------

#[test]
fn activate_appends_creates_one_backup_and_preserves_crlf_and_version() {
    let scratch = scratch();

    // This test operates on a scratch copy (never the real game's own
    // `ModsConfig.xml`), so `--force` only bypasses the running-game
    // probe — it never bypasses a safety check this codebase actually
    // needs here (see this file's own module doc comment: that refusal
    // is covered by `commands::mods`'s `FixedProbe` unit tests).
    scratch
        .cmd(&["activate", "aaa.mod", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("wrote ModsConfig.xml"))
        .stdout(predicates::str::contains(
            "next: run 'rimmerge sort' then 'rimmerge apply' to place them",
        ));

    let bytes = scratch.read_mods_config();
    let text = String::from_utf8(bytes).expect("utf8");
    assert!(text.contains("\r\n"), "CRLF line endings must be preserved");
    assert!(text.contains("<version>1.6.0</version>"));
    // Appended at the end, after the already-active sample.mod.
    let sample_pos = text
        .find("<li>sample.mod</li>")
        .expect("sample.mod present");
    let aaa_pos = text.find("<li>aaa.mod</li>").expect("aaa.mod present");
    assert!(
        sample_pos < aaa_pos,
        "aaa.mod must be appended after sample.mod"
    );

    assert_eq!(
        scratch.backup_files().len(),
        1,
        "exactly one .bak- file must be created"
    );
}

#[test]
fn activate_with_dependencies_pulls_in_the_declared_dependency_first() {
    let scratch = scratch();
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("AaaMod"),
        "aaa.mod",
        "Aaa Mod",
        "zzz.mod",
    );

    scratch
        .cmd(&["activate", "aaa.mod", "--with-dependencies", "--force"])
        .assert()
        .success();

    let bytes = scratch.read_mods_config();
    let text = String::from_utf8(bytes).expect("utf8");
    let zzz_pos = text.find("<li>zzz.mod</li>").expect("zzz.mod activated");
    let aaa_pos = text.find("<li>aaa.mod</li>").expect("aaa.mod activated");
    assert!(
        zzz_pos < aaa_pos,
        "the dependency (zzz.mod) must be activated before its dependent (aaa.mod)"
    );
}

/// The other half of the closure test above: the exact same
/// declared-dependency fixture, but omitting
/// `--with-dependencies` — a hardcoded `plan_activate(.., true)` bug
/// would pass the test above and still pass here, since both would
/// activate zzz.mod either way.
#[test]
fn activate_without_with_dependencies_leaves_the_declared_dependency_inactive() {
    let scratch = scratch();
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("AaaMod"),
        "aaa.mod",
        "Aaa Mod",
        "zzz.mod",
    );

    scratch
        .cmd(&["activate", "aaa.mod", "--force"])
        .assert()
        .success();

    let text = String::from_utf8(scratch.read_mods_config()).expect("utf8");
    assert!(
        text.contains("<li>aaa.mod</li>"),
        "aaa.mod itself activates"
    );
    assert!(
        !text.contains("<li>zzz.mod</li>"),
        "without --with-dependencies, the declared dependency must stay inactive:\n{text}"
    );
}

/// A declared dependency naming an id with no folder on disk at all is
/// reported, never added — the requested mod itself still activates.
#[test]
fn activate_with_dependencies_reports_an_unresolvable_dependency() {
    let scratch = scratch();
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("AaaMod"),
        "aaa.mod",
        "Aaa Mod",
        "ghost.framework",
    );

    scratch
        .cmd(&["activate", "aaa.mod", "--with-dependencies", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "unresolvable dependencies (not on disk)",
        ))
        .stdout(predicates::str::contains("ghost.framework"));

    let text = String::from_utf8(scratch.read_mods_config()).expect("utf8");
    assert!(
        text.contains("<li>aaa.mod</li>"),
        "aaa.mod itself activates"
    );
    assert!(
        !text.contains("ghost.framework"),
        "an unresolvable dependency is never added:\n{text}"
    );
}

/// `--json --dry-run` reports the plan shape for `activate`.
#[test]
fn activate_with_dependencies_json_dry_run_reports_the_plan_shape() {
    let scratch = scratch();
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("AaaMod"),
        "aaa.mod",
        "Aaa Mod",
        "zzz.mod",
    );

    let output = scratch
        .cmd(&[
            "activate",
            "aaa.mod",
            "--with-dependencies",
            "--json",
            "--dry-run",
        ])
        .output()
        .unwrap_or_else(|error| panic!("run mods activate --json: {error}"));
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON on stdout");

    assert_eq!(json["will_activate"], serde_json::json!(["aaa.mod"]));
    assert_eq!(
        json["dependencies_also_activated"],
        serde_json::json!(["zzz.mod"])
    );
    assert!(
        json.get("backup_path").is_none(),
        "a dry run must carry no backup_path: {json}"
    );
}

#[test]
fn activate_dry_run_writes_nothing_and_creates_no_backup() {
    let scratch = scratch();
    let before = scratch.read_mods_config();

    scratch
        .cmd(&["activate", "aaa.mod", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dry run — nothing written"));

    assert_eq!(scratch.read_mods_config(), before, "dry run must not write");
    assert!(
        scratch.backup_files().is_empty(),
        "dry run must not back up"
    );
}

#[test]
fn activate_an_unknown_id_exits_non_zero_and_writes_nothing() {
    let scratch = scratch();
    let before = scratch.read_mods_config();

    scratch
        .cmd(&["activate", "does.not.exist"])
        .assert()
        .failure();

    assert_eq!(
        scratch.read_mods_config(),
        before,
        "an unknown id must leave ModsConfig.xml byte-identical"
    );
    assert!(scratch.backup_files().is_empty());
}

// -- deactivate -------------------------------------------------------------

#[test]
fn deactivate_the_only_active_mod_empties_the_active_list() {
    let scratch = scratch();

    scratch
        .cmd(&["deactivate", "sample.mod", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("wrote ModsConfig.xml"));

    let bytes = scratch.read_mods_config();
    let text = String::from_utf8(bytes).expect("utf8");
    assert!(!text.contains("<li>sample.mod</li>"));
}

/// Also pins the *exact* refusal wording, not just failure + bytes — a
/// mutant that printed the
/// `--dry-run` message ("(dry run — nothing written)") for this genuine,
/// non-dry-run refusal instead of `WriteOutcome::RefusedCore`'s own text
/// would otherwise still pass.
#[test]
fn deactivate_core_is_refused_and_writes_nothing() {
    let scratch = scratch();
    let before = scratch.read_mods_config();

    scratch
        .cmd(&["deactivate", "ludeon.rimworld"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("(refused — nothing written)"));

    assert_eq!(scratch.read_mods_config(), before);
    assert!(scratch.backup_files().is_empty());
}

#[test]
fn deactivate_requires_yes_when_a_declared_dependent_is_still_active() {
    let scratch = scratch();
    // Both sample.mod and aaa.mod active; sample.mod declares aaa.mod a
    // dependency.
    fs::write(&scratch.mods_config,
        "<?xml version=\"1.0\" ?>\r\n<ModsConfigData>\r\n  <version>1.6.0</version>\r\n  <activeMods>\r\n    <li>sample.mod</li>\r\n    <li>aaa.mod</li>\r\n  </activeMods>\r\n</ModsConfigData>\r\n")
    .expect("seed a two-mod ModsConfig.xml");
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("SampleMod"),
        "sample.mod",
        "Sample Mod",
        "aaa.mod",
    );
    let before = scratch.read_mods_config();

    // Without --yes: refused, nothing written. Both the dependents block
    // and the outcome-specific trailing message are pinned — a
    // mutant printing "(dry run — nothing written)" here instead of the
    // blocked-by-dependents message would otherwise still pass.
    scratch
        .cmd(&["deactivate", "aaa.mod"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("dependents (declared)"))
        .stdout(predicates::str::contains("blocked by active dependents"));
    assert_eq!(scratch.read_mods_config(), before);
    assert!(scratch.backup_files().is_empty());

    // With --yes: honoured, aaa.mod is removed.
    scratch
        .cmd(&["deactivate", "aaa.mod", "--yes", "--force"])
        .assert()
        .success();
    let text = String::from_utf8(scratch.read_mods_config()).expect("utf8");
    assert!(!text.contains("<li>aaa.mod</li>"));
    assert!(text.contains("<li>sample.mod</li>"));
}

/// `--json` reports the dependents block for `deactivate`.
#[test]
fn deactivate_json_reports_the_dependents_block() {
    let scratch = scratch();
    set_active_mods(&scratch, &["sample.mod", "aaa.mod"]);
    add_mod_dependency(
        &scratch.game_dir.join("Mods").join("SampleMod"),
        "sample.mod",
        "Sample Mod",
        "aaa.mod",
    );

    let output = scratch
        .cmd(&["deactivate", "aaa.mod", "--json"])
        .output()
        .unwrap_or_else(|error| panic!("run mods deactivate --json: {error}"));
    assert!(
        !output.status.success(),
        "a dependents block without --yes must exit non-zero"
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON on stdout");

    assert_eq!(json["will_deactivate"], serde_json::json!(["aaa.mod"]));
    assert_eq!(
        json["dependents_declared"]["aaa.mod"],
        serde_json::json!(["sample.mod"])
    );
    assert!(
        json.get("backup_path").is_none(),
        "a blocked deactivate must carry no backup_path: {json}"
    );
}
