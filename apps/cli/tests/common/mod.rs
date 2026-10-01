//! Shared fixture helpers for this crate's `assert_cmd` end-to-end tests
//! (`apply_dry_run.rs`, `merge_cli.rs`, `patch_cli.rs`) plus the
//! real-install guard shared by every `real_install_*.rs` test in this
//! crate (`real_install_assign.rs`, `real_install_defs.rs`,
//! `real_install_merge_coverage.rs`, `real_install_pair_rules.rs`,
//! `real_install_def_suffix_exemption.rs`). A separate compilation unit
//! from `src/`, so this can't reach `src/test_fixtures.rs` (that crate's
//! own `#[cfg(test)]` unit-test equivalent) — each gets its own copy.
//!
//! **The real-install tier is `RIMMERGE_GAME_DIR`-driven.** There is no
//! hardcoded install path here or anywhere else in the workspace; "does
//! this machine have the install these tests are written against" is
//! exactly "`RIMMERGE_GAME_DIR` is set and
//! [`rim_analyzer::infra::paths::is_game_dir`] agrees", the same
//! predicate detection itself uses.
//!
//! Two vacuous-green hazards are guarded here:
//!
//! 1. **An unset `RIMMERGE_PERF_PROFILE_DIR`** must never soft-skip, or
//!    `--run-ignored ignored-only` would report the whole tier *passed*
//!    while asserting nothing — see `docs/testing.md`'s three-state rule
//!    for why every guard here panics instead.
//! 2. **A trivial active mod list.** The running game can rewrite the live
//!    `ModsConfig.xml` down to just its DLCs, and a test like
//!    `verify_order_against_the_real_install` then *passes* in a fraction
//!    of its usual time — every assertion it makes holds vacuously over an
//!    empty cross-mod set. [`MIN_ACTIVE_MODS`] closes it: an install in
//!    this tier has hundreds of active mods, and anything less is a loud
//!    failure, not a green tick.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use rim_session::ProjectPaths;

/// A `rimmerge` command plus the app-global base directory it runs against.
/// The base is deleted when this value drops, so a test run leaves nothing
/// behind; it dereferences to the [`Command`] so a call site chains `.args(..)`
/// and `.assert()` as before.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module spawns the CLI"
)]
pub struct CliCommand {
    command: Command,
    _base: tempfile::TempDir,
}

impl std::ops::Deref for CliCommand {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.command
    }
}

impl std::ops::DerefMut for CliCommand {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.command
    }
}

/// A `rimmerge` command whose app-global base (`RIMMERGE_PROFILE_DIR`:
/// `app-settings.json`, `notifications.json`, the rule-database cache, and
/// the default `profiles/` parent) is a fresh, empty directory under
/// `CARGO_TARGET_TMPDIR`. Without it a CLI test reads the developer's real
/// `%LOCALAPPDATA%\rimmerge` (their network switches, a fetched rules
/// cache) and its result depends on the machine it runs on. A test that
/// needs a specific base sets `RIMMERGE_PROFILE_DIR` itself afterwards; the
/// later `.env` wins.
///
/// Every call gets its own base, so parallel tests never share state, and the
/// returned [`CliCommand`] removes it on drop.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module spawns the CLI"
)]
pub fn rimmerge() -> CliCommand {
    let parent = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-app-base");
    fs::create_dir_all(&parent)
        .unwrap_or_else(|error| panic!("the app-base parent must be creatable: {error}"));
    let base = tempfile::Builder::new()
        .prefix("base-")
        .tempdir_in(&parent)
        .unwrap_or_else(|error| panic!("the app base must be creatable: {error}"));
    let mut command = Command::cargo_bin("rimmerge")
        .unwrap_or_else(|error| panic!("the rimmerge binary must build: {error}"));
    command.env("RIMMERGE_PROFILE_DIR", base.path());
    CliCommand {
        command,
        _base: base,
    }
}

/// The floor an install has to clear to count as "the real-install tier's
/// install". Deliberately a few hundred rather than a number tied to
/// today's list: the point is to separate a real modded install from a
/// vanilla one (a handful of active mods — Core and the DLCs), not to pin
/// a count that drifts with every subscription.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub const MIN_ACTIVE_MODS: usize = 200;

/// True when `RIMMERGE_GAME_DIR` is set **and** names a real install
/// ([`rim_analyzer::infra::paths::is_game_dir`] — `Version.txt` and
/// `Data/Core/` both present).
///
/// Only a *query*, for a caller that wants to branch without the
/// skip/panic behaviour. It deliberately answers `false` for both "unset"
/// and "set to something that isn't an install" — the two states
/// [`require_game_dir`] draws a hard line between — so nothing that has
/// to tell them apart may use it.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_install_is_present() -> bool {
    rim_analyzer::infra::paths::path_var(rim_io::GAME_DIR_VAR)
        .is_some_and(|dir| rim_analyzer::infra::paths::is_game_dir(&dir))
}

/// `RIMMERGE_GAME_DIR`, or `None` after an honest `skipping:` line when
/// it isn't set at all.
///
/// A variable that *is* set but doesn't name an install **panics**: that
/// is a misconfigured run, not a machine outside this tier, and silently
/// skipping it is how a typo'd path turns a whole tier green — the exact
/// shape of the hazard this module exists to close.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn require_game_dir(rerun_command: &str) -> Option<PathBuf> {
    let var = rim_io::GAME_DIR_VAR;
    match rim_analyzer::infra::paths::path_var(var) {
        Some(dir) if rim_analyzer::infra::paths::is_game_dir(&dir) => Some(dir),
        Some(dir) => panic!(
            "{var} is set to {} but that is not a RimWorld install (no Version.txt and \
             Data/Core/). A typo here must fail loudly, not skip the tier. Point it at your \
             install directory. Run: {rerun_command}",
            dir.display()
        ),
        None => {
            eprintln!(
                "skipping: {var} is not set, so this machine is not in the real-install tier. \
                 Run: {rerun_command}"
            );
            None
        }
    }
}

/// The tier's gate, and every real-install test's first line: the
/// scratch profile directory, or `None` after an honest `skipping:` line
/// when `RIMMERGE_GAME_DIR` is **unset**.
///
/// Delegates the install question to [`require_game_dir`] rather than to
/// [`real_install_is_present`], and that is load-bearing: the boolean
/// collapses "unset" and "set to a typo" into one `false`, so routing
/// through it made `require_game_dir`'s panic branch unreachable from
/// every test and turned a mistyped variable back into a silent
/// tier-wide skip. Two distinct states, two distinct behaviours.
///
/// Inside the tier, an unset `RIMMERGE_PERF_PROFILE_DIR` panics too: a
/// real-install test needs a scratch profile to do anything, so a soft
/// skip there is the vacuous-pass trap this module's own doc comment
/// calls out. `rerun_command` is the caller's own already-documented
/// "Run with" invocation, named in every message so it is actionable for
/// that specific tier.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn require_profile_dir(rerun_command: &str) -> Option<PathBuf> {
    require_game_dir(rerun_command)?;
    match std::env::var("RIMMERGE_PERF_PROFILE_DIR") {
        Ok(dir) if !dir.trim().is_empty() => Some(PathBuf::from(dir)),
        _ => panic!(
            "RIMMERGE_PERF_PROFILE_DIR not set, but {} names a real RimWorld install -- this \
             test must fail loudly here, not soft-skip (see docs/testing.md's three-state \
             rule). Run: {rerun_command}",
            rim_io::GAME_DIR_VAR
        ),
    }
}

/// A single named env var carrying one real-install pin/ground-truth
/// value: `None` (after
/// an honest skip message) on a machine outside the real-install tier;
/// **panics**, naming `var`, on a machine inside it with `var` unset —
/// the same three-state rule [`require_profile_dir`] already applies to
/// `RIMMERGE_PERF_PROFILE_DIR`, generalized to any test-specific pin
/// variable. A test whose whole body sits behind its own pin var calls
/// this (and, usually, [`require_profile_dir`]) before doing anything
/// else, rather than soft-skipping unconditionally — a soft skip inside
/// the tier is exactly the vacuous-green shape this module's own doc
/// comment exists to close.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn require_pin_var(var: &str, rerun_hint: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(value) => Some(value),
        Err(_) if real_install_is_present() => {
            panic!(
                "{var} is not set, but this machine is in the real-install tier -- this test \
                 needs it. Run: {rerun_hint}"
            )
        }
        Err(_) => {
            eprintln!("skipping: {var} is not set. Run: {rerun_hint}");
            None
        }
    }
}

/// The real install's own paths for `profile_dir`, with the active-mod
/// floor asserted — every `real_install_*.rs` test's `load_real_session`
/// goes through this instead of building a [`ProjectPaths`] by hand, so
/// the floor can never be forgotten on a new test the way it was on
/// every existing one.
///
/// Panics rather than returning `None`: by the time a test calls this it
/// has already passed [`require_profile_dir`], so the install *is*
/// present and anything wrong from here on is a misconfiguration to
/// shout about.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_install_paths(profile_dir: PathBuf, rerun_command: &str) -> ProjectPaths {
    let game_dir = require_game_dir(rerun_command).unwrap_or_else(|| {
        panic!(
            "{} must be set by the time a real-install test builds its paths -- \
             require_profile_dir should already have skipped. Run: {rerun_command}",
            rim_io::GAME_DIR_VAR
        )
    });
    let mods_config = real_mods_config_path();
    assert_active_mod_floor(&mods_config, rerun_command);
    ProjectPaths {
        workshop_dir: real_workshop_dir(&game_dir),
        mods_config,
        game_dir,
        profile_dir,
    }
}

/// `RIMMERGE_MODS_CONFIG`, else the `%USERPROFILE%`-relative default.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_mods_config_path() -> PathBuf {
    rim_analyzer::infra::paths::path_var(rim_io::MODS_CONFIG_VAR).unwrap_or_else(|| {
        rim_analyzer::infra::paths::default_mods_config_path()
            .unwrap_or_else(|error| panic!("resolving ModsConfig.xml path: {error}"))
    })
}

/// `RIMMERGE_WORKSHOP_DIR`, else the derivation off `game_dir`.
///
/// Honoured for the same reason `RIMMERGE_MODS_CONFIG` is: an install
/// whose Workshop content isn't two levels up from `steamapps/common`
/// (a moved library, a hand-assembled tree) would otherwise have this
/// tier silently scanning an empty folder — which shows up as *fewer*
/// findings, the failure mode hardest to notice.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_workshop_dir(game_dir: &Path) -> PathBuf {
    rim_analyzer::infra::paths::path_var(rim_io::WORKSHOP_DIR_VAR)
        .unwrap_or_else(|| rim_analyzer::infra::paths::default_workshop_dir(game_dir))
}

/// Panics unless `mods_config` lists at least [`MIN_ACTIVE_MODS`] active
/// mods — see hazard 2 in this module's own doc comment.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn assert_active_mod_floor(mods_config: &Path, rerun_command: &str) {
    let bytes = fs::read(mods_config)
        .unwrap_or_else(|error| panic!("reading {}: {error}", mods_config.display()));
    let document = rim_analyzer::extract::mods_config::parse_mods_config(&bytes)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", mods_config.display()));
    let active = document.active_mods.len();
    assert!(
        active >= MIN_ACTIVE_MODS,
        "this is the real-install tier and your install is not in it -- {} lists {active} active \
         mods, below the {MIN_ACTIVE_MODS} floor. A vanilla or reset ModsConfig.xml makes every \
 cross-mod assertion here hold vacuously (own OPEN entry: \
         verify_order_against_the_real_install once passed in 1.256s over 6 active mods). \
         Restore your real mod list, or point {} at the install this tier is for. Run: \
         {rerun_command}",
        mods_config.display(),
        rim_io::MODS_CONFIG_VAR
    );
}

/// `crates/rim-io/tests/fixtures/merge_game` — the two-mod fixture with a
/// contested `ThingDef` override, shared by `merge_cli.rs` and
/// `patch_cli.rs`.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn merge_game_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-io")
        .join("tests")
        .join("fixtures")
        .join("merge_game")
}

/// Recursively copies `src` into `dst`, creating directories as needed —
/// every test that needs a scratch, disposable game tree starts from a
/// copy of a checked-in fixture rather than touching it in place.
///
/// Not a test function itself (so `clippy::expect_used`'s test allowance
/// doesn't apply here) — propagates via `?` and lets each `#[test]`
/// caller `.expect()`/`unwrap_or_else` the result instead.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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
