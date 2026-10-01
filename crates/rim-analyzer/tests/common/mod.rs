//! The real-install guard shared by this crate's `#[ignore]`d
//! `real_install_*.rs` tests and `pe_metadata_ground_truth.rs` — the
//! third copy of the pair `apps/cli/tests/common/mod.rs` and
//! `apps/desktop/src-tauri/src/real_install_support.rs` already hold
//! (three crates, three compilation units, no shared test-only crate;
//! kept in sync by hand).
//!
//! **The tier is `RIMMERGE_GAME_DIR`-driven** — no install path is hardcoded.
//! `RIMMERGE_PERF_PROFILE_DIR` is still required even though this crate's own
//! scan has no profile concept at all: it is the tier's shared safety rail
//! (root `CLAUDE.md`), and a test here that ran without it would be the odd
//! one out in a documented run command that sets it.
//!
//! Two vacuous-green hazards are guarded against here (see `docs/testing.md`'s
//! three-state rule): an unset variable must never soft-skip a whole tier
//! into a green tick, and a *vanilla* `ModsConfig.xml` must not pass every
//! cross-mod assertion vacuously either. [`MIN_ACTIVE_MODS`]
//! closes the second.

use std::fs;
use std::path::PathBuf;

use rim_analyzer::domain::{FolderPolicy, GameVersion};
use rim_analyzer::extract::mods_config::parse_mods_config;
use rim_analyzer::infra::{ScanConfig, paths};

/// See the identically-named constants in the other two guard copies.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub const MIN_ACTIVE_MODS: usize = 200;

/// True when `RIMMERGE_GAME_DIR` is set **and** names a real install
/// ([`paths::is_game_dir`]). Only a query, for [`require_pin_var`]'s own
/// branch — never a substitute for [`require_game_dir`], since it
/// collapses "unset" and "set to a typo" into one `false`.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_install_is_present() -> bool {
    paths::path_var(paths::GAME_DIR_VAR).is_some_and(|dir| paths::is_game_dir(&dir))
}

/// A single pinned ground-truth value read from an env var: unset outside the
/// tier is an honest soft skip; unset **inside** the tier panics, naming the
/// var and `rerun_hint` — the shared three-state shape, so no pin-var site in
/// this crate's `real_install_*.rs` tests hand-rolls an ad-hoc `let Ok(raw) =
/// std::env::var(..) else { eprintln!(...); return; }` (two states only, no
/// panic branch).
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

/// `RIMMERGE_GAME_DIR`, or `None` after an honest `skipping:` line when
/// it isn't set. A variable that *is* set but doesn't name an install
/// **panics**: a misconfigured run, not a machine outside the tier.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn require_game_dir(rerun_command: &str) -> Option<PathBuf> {
    let var = paths::GAME_DIR_VAR;
    match paths::path_var(var) {
        Some(dir) if paths::is_game_dir(&dir) => Some(dir),
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

/// The tier's shared `RIMMERGE_PERF_PROFILE_DIR` rail: panics when it is
/// unset on a machine that *is* in the tier (a `RIMMERGE_GAME_DIR` naming
/// a real install). Call it after [`require_game_dir`] has already
/// returned `Some`, so a machine outside the tier skips quietly first.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn require_scratch_profile_dir_is_set(rerun_command: &str) {
    let set =
        std::env::var("RIMMERGE_PERF_PROFILE_DIR").is_ok_and(|value| !value.trim().is_empty());
    assert!(
        set,
        "RIMMERGE_PERF_PROFILE_DIR not set, but RIMMERGE_GAME_DIR names a real RimWorld \
         install. Point it at a scratch directory (never the real RimSort profile); this crate's \
         own tests write nothing to it, but the workspace's real-install tier convention \
         requires every test in it to fail loudly rather than silently skip. Run: {rerun_command}"
    );
}

/// `RIMMERGE_MODS_CONFIG`, else the `%USERPROFILE%`-relative default.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_mods_config_path() -> PathBuf {
    paths::path_var(paths::MODS_CONFIG_VAR).unwrap_or_else(|| {
        paths::default_mods_config_path()
            .unwrap_or_else(|error| panic!("resolving ModsConfig.xml path: {error}"))
    })
}

/// `RIMMERGE_WORKSHOP_DIR`, else the derivation off `game_dir` — honoured
/// for the same reason `RIMMERGE_MODS_CONFIG` is: an install whose
/// Workshop content isn't two levels up from `steamapps/common` would
/// otherwise have this tier silently scanning an empty folder, which
/// surfaces as *fewer* findings rather than an error.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_workshop_dir(game_dir: &std::path::Path) -> PathBuf {
    paths::path_var(paths::WORKSHOP_DIR_VAR)
        .unwrap_or_else(|| paths::default_workshop_dir(game_dir))
}

/// A read-only [`ScanConfig`] over the real install, with both hazard
/// guards applied — `None` only on a machine outside the tier.
///
/// Returns the resolved [`GameVersion`] alongside it: two of this crate's
/// four real-install tests need it, and re-reading `Version.txt` in each
/// of them would be a second place for the resolution rule to drift.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn real_scan_config(rerun_command: &str) -> Option<(ScanConfig, GameVersion)> {
    let game_dir = require_game_dir(rerun_command)?;
    require_scratch_profile_dir_is_set(rerun_command);
    let mods_config_path = real_mods_config_path();
    assert_active_mod_floor(&mods_config_path, rerun_command);
    let game_version = paths::default_game_version(&game_dir)
        .unwrap_or_else(|error| panic!("reading the real install's game version: {error}"));
    Some((
        ScanConfig {
            workshop_dir: real_workshop_dir(&game_dir),
            mods_config_path,
            game_dir,
            game_version,
            folder_policy: FolderPolicy::LoadFolders,
            active_mods: None,
        },
        game_version,
    ))
}

/// Panics unless `mods_config` lists at least [`MIN_ACTIVE_MODS`] active
/// mods — the second hazard in this module's own doc comment.
#[allow(
    dead_code,
    reason = "not every test binary that includes this module uses every helper"
)]
pub fn assert_active_mod_floor(mods_config: &std::path::Path, rerun_command: &str) {
    let bytes = fs::read(mods_config)
        .unwrap_or_else(|error| panic!("reading {}: {error}", mods_config.display()));
    let document = parse_mods_config(&bytes)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", mods_config.display()));
    let active = document.active_mods.len();
    assert!(
        active >= MIN_ACTIVE_MODS,
        "this is the real-install tier and your install is not in it -- {} lists {active} active \
         mods, below the {MIN_ACTIVE_MODS} floor. A vanilla or reset ModsConfig.xml makes every \
 cross-mod assertion here hold vacuously. \
         Restore your real mod list, or point RIMMERGE_MODS_CONFIG at the install this tier is \
         for. Run: {rerun_command}",
        mods_config.display()
    );
}
