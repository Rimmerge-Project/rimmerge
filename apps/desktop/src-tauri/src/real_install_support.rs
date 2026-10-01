//! Shared real-install guard for this crate's `#[ignore]`d real-install
//! tests (`real_install_timing.rs`, `def_inspector_timing.rs`) — the
//! desktop-crate counterpart of `apps/cli/tests/common/mod.rs`'s
//! identically-named set. A separate, small module rather than folding
//! into `test_support.rs`: that module is this crate's general-purpose
//! `Session`-builder helpers for ordinary command unit tests, not the
//! real-install ignored tier's own hazard guard. A separate *copy* rather
//! than a dependency on the CLI's test-only module, because that one
//! lives in a different crate's `tests/`.
//!
//! **The real-install tier is `RIMMERGE_GAME_DIR`-driven**
//! — no hardcoded install
//! path survives anywhere in the workspace. Two vacuous-green hazards are
//! guarded against here (see `docs/testing.md`'s three-state rule): an
//! unset `RIMMERGE_PERF_PROFILE_DIR` must never soft-skip the whole tier
//! into a green tick, and a *vanilla* `ModsConfig.xml` must not pass every
//! cross-mod assertion vacuously either — `verify_order_against_the_real_install`,
//! in `real_install_timing.rs`, takes ~60 s per order source on the real list, so a
//! run over a handful of active mods finishing in about a second is the
//! tell that the floor below was needed.
//! [`MIN_ACTIVE_MODS`] closes the second.

use std::fs;
use std::path::{Path, PathBuf};

use rim_session::ProjectPaths;

/// See `apps/cli/tests/common/mod.rs`'s constant of the same name — kept
/// in sync by hand, like the rest of this module.
pub(crate) const MIN_ACTIVE_MODS: usize = 200;

/// True when `RIMMERGE_GAME_DIR` is set **and** names a real install. Only
/// a query, for a caller (namely [`require_pin_var`]) that wants to
/// branch without the skip/panic behaviour [`require_game_dir`] gives —
/// never a substitute for it, since it collapses "unset" and "set to a
/// typo" into one `false`.
pub(crate) fn real_install_is_present() -> bool {
    rim_analyzer::infra::paths::path_var(rim_io::GAME_DIR_VAR)
        .is_some_and(|dir| rim_analyzer::infra::paths::is_game_dir(&dir))
}

/// A single pinned ground-truth value read from an env var: unset outside the
/// tier is an honest soft skip (the caller falls back to a shape/count-only
/// assertion); unset **inside** the tier panics, naming the var and
/// `rerun_hint` — the same three-state rule every other real-install guard
/// in this module already follows, generalized for the many single-value
/// pins (`pe_metadata_ground_truth.rs`,
/// `real_install_selected_classes.rs`, `real_install_defs.rs`) that used
/// to each hand-roll this exact `let Ok(raw) = std::env::var(..) else {
/// eprintln!(...); return; }` two-state shape.
pub(crate) fn require_pin_var(var: &str, rerun_hint: &str) -> Option<String> {
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
/// **panics**: that is a misconfigured run, not a machine outside the
/// tier, and skipping a typo is how the whole tier goes green while
/// asserting nothing.
pub(crate) fn require_game_dir(rerun_command: &str) -> Option<PathBuf> {
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
/// any "is an install present" boolean, and that is load-bearing: such a
/// boolean collapses "unset" and "set to a typo" into one `false`, which
/// made the panic branch above unreachable from every test here and
/// turned a mistyped variable into a silent tier-wide skip. Two distinct
/// states, two distinct behaviours. On a machine that *is* in the tier an
/// unset `RIMMERGE_PERF_PROFILE_DIR` panics too — see this module's own
/// doc comment.
pub(crate) fn require_profile_dir(rerun_command: &str) -> Option<PathBuf> {
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

/// The real install's own paths for `profile_dir`, with the active-mod
/// floor asserted — every real-install test here builds its
/// [`ProjectPaths`] through this rather than by hand, so the floor can
/// never be forgotten on a new one.
///
/// Panics rather than returning `None`: by the time a test calls this it
/// has already passed [`require_profile_dir`].
pub(crate) fn real_install_paths(profile_dir: PathBuf, rerun_command: &str) -> ProjectPaths {
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
fn real_mods_config_path() -> PathBuf {
    rim_analyzer::infra::paths::path_var(rim_io::MODS_CONFIG_VAR).unwrap_or_else(|| {
        rim_analyzer::infra::paths::default_mods_config_path()
            .unwrap_or_else(|error| panic!("resolving ModsConfig.xml path: {error}"))
    })
}

/// `RIMMERGE_WORKSHOP_DIR`, else the derivation off `game_dir` — honoured
/// for the same reason `RIMMERGE_MODS_CONFIG` is: an install whose
/// Workshop content isn't two levels up from `steamapps/common` would
/// otherwise have this tier silently scanning an empty folder, which
/// surfaces as *fewer* findings rather than an error.
fn real_workshop_dir(game_dir: &Path) -> PathBuf {
    rim_analyzer::infra::paths::path_var(rim_io::WORKSHOP_DIR_VAR)
        .unwrap_or_else(|| rim_analyzer::infra::paths::default_workshop_dir(game_dir))
}

/// Panics unless `mods_config` lists at least [`MIN_ACTIVE_MODS`] active
/// mods — the second hazard in this module's own doc comment.
fn assert_active_mod_floor(mods_config: &Path, rerun_command: &str) {
    let bytes = fs::read(mods_config)
        .unwrap_or_else(|error| panic!("reading {}: {error}", mods_config.display()));
    let document = rim_analyzer::extract::mods_config::parse_mods_config(&bytes)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", mods_config.display()));
    let active = document.active_mods.len();
    assert!(
        active >= MIN_ACTIVE_MODS,
        "this is the real-install tier and your install is not in it -- {} lists {active} active \
         mods, below the {MIN_ACTIVE_MODS} floor. A vanilla or reset ModsConfig.xml makes every \
         cross-mod assertion here hold vacuously (previously demonstrated: \
         verify_order_against_the_real_install once passed in 1.256s over 6 active mods). \
         Restore your real mod list, or point {} at the install this tier is for. Run: \
         {rerun_command}",
        mods_config.display(),
        rim_io::MODS_CONFIG_VAR
    );
}
