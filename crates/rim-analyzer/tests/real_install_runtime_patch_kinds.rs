//! Measurement: the real install's runtime patch-kind distribution, and
//! specifically whether the collision that motivates the transpiler finding
//! (`Verse.Verb_LaunchProjectile.TryCastShot`) has two or more distinct mods
//! each declaring a `Transpiler` on it. This test asserts the *class* of
//! result (kinds are extracted at all, `Unknown` isn't universal, the
//! documented collision still exists) and reports the actual counts via
//! `eprintln!`, never pinning exact figures, which drift with Workshop
//! content.
//!
//! `#[ignore]`d: reads the real game/workshop install and the real
//! `ModsConfig.xml` (read-only — nothing here writes anywhere) and
//! requires `RIMMERGE_PERF_PROFILE_DIR` to be set. This crate's own scan
//! needs no profile directory at all (that's a `rim-session`/`rim-io`
//! concept), so `common::require_scratch_profile_dir_is_set` is a pure
//! safety rail matching every other test in this documented tier (root
//! `CLAUDE.md`, `real_install_load_folders.rs`) — it **fails loudly, by
//! panicking**, when the variable is unset, rather than soft-skipping.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::{HashMap, HashSet};

use rim_analyzer::analysis::{self, RunContext};
use rim_analyzer::domain::{Conflict, ModId, Report, RuntimePatchKind, ScanOutput};
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// Scans the real, current install and runs the full analyzer over it,
/// read-only, keeping the raw [`ScanOutput`] alongside the built
/// [`Report`] — `Report.mods` never carries `runtime_patches` (that lives
/// on `ScannedMod`, not the `Mod` a `Report` serializes), so this
/// measurement needs both. Panics with a clear message on any failure —
/// this test only ever runs by hand, against this specific development
/// machine, so a loud panic is more useful here than threading a `Result`
/// through a `#[test]` fn.
fn scan_and_analyze_real_install() -> Option<(ScanOutput, Report)> {
    let (config, game_version) = common::real_scan_config(RERUN_COMMAND)?;
    let scan =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));
    let context = RunContext {
        game_dir: config.game_dir,
        workshop_dir: config.workshop_dir,
        mods_config_path: config.mods_config_path,
        game_version,
    };
    let report = analysis::build_ref(&scan, &context);
    Some((scan, report))
}

/// Every mod id that declares a runtime patch of `kind` on
/// `(type_name, method_name)`, from the raw scan — this test's own
/// independent oracle over `ScannedMod::assemblies` rather than a call
/// into `analysis::indices`/`analysis::conflicts`' own machinery, which is
/// exactly what this measurement is checking the *inputs* to.
fn owners_declaring_kind(
    scan: &ScanOutput,
    type_name: &str,
    method_name: &str,
    kind: RuntimePatchKind,
) -> HashSet<ModId> {
    scan.scanned_mods
        .iter()
        .filter(|scanned_mod| {
            scanned_mod.assemblies.iter().any(|assembly| {
                assembly.runtime_patches.iter().any(|patch| {
                    patch.kind == kind
                        && patch.type_name == type_name
                        && patch.method_name == method_name
                })
            })
        })
        .map(|scanned_mod| scanned_mod.info.id.clone())
        .collect()
}

/// The measurement, in full: the kind distribution across every extracted
/// runtime patch target, and — for every `RuntimePatchCollision` the analyzer
/// already reports — how many involve two or more distinct mods each
/// declaring a `Transpiler` on the exact same target. That second number is
/// expected to stay narrow (a small fraction of all reported collisions); a
/// result in the hundreds would mean the transpiler finding's premise is
/// wrong.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn runtime_patch_kind_distribution_and_transpiler_collision_count() {
    let Some((scan, report)) = scan_and_analyze_real_install() else {
        return;
    };

    let mut kind_counts: HashMap<RuntimePatchKind, usize> = HashMap::new();
    for scanned_mod in &scan.scanned_mods {
        for assembly in &scanned_mod.assemblies {
            for patch in &assembly.runtime_patches {
                *kind_counts.entry(patch.kind).or_insert(0) += 1;
            }
        }
    }
    let total_patches: usize = kind_counts.values().sum();
    let unknown_count = kind_counts
        .get(&RuntimePatchKind::Unknown)
        .copied()
        .unwrap_or(0);

    eprintln!(
        "runtime-patch-kind measurement: {total_patches} total extracted runtime patch targets"
    );
    for kind in [
        RuntimePatchKind::Prefix,
        RuntimePatchKind::Postfix,
        RuntimePatchKind::Transpiler,
        RuntimePatchKind::Unknown,
    ] {
        eprintln!(
            "  {kind:?}: {}",
            kind_counts.get(&kind).copied().unwrap_or(0)
        );
    }

    assert!(
        total_patches > 0,
        "no runtime patches extracted at all on this install — the measurement can't run"
    );
    assert!(
        unknown_count < total_patches,
        "every extracted runtime patch resolved to Unknown ({unknown_count}/{total_patches}) — \
         the attribute- and convention-based kind extraction looks broken or entirely missing \
         against this install's real assemblies"
    );

    let collisions: Vec<_> = report
        .conflicts
        .iter()
        .filter_map(|conflict| match conflict {
            Conflict::RuntimePatchCollision(collision) => Some(collision),
            _ => None,
        })
        .collect();

    let mut transpiler_vs_transpiler = 0usize;
    for collision in &collisions {
        let transpiler_owners = owners_declaring_kind(
            &scan,
            &collision.target_type,
            &collision.target_method,
            RuntimePatchKind::Transpiler,
        );
        if transpiler_owners.len() >= 2 {
            transpiler_vs_transpiler += 1;
        }
    }

    eprintln!(
        "runtime-patch-kind measurement: {transpiler_vs_transpiler} of {} RuntimePatchCollisions involve two or \
         more distinct mods each declaring a Transpiler on the same target (expected to be a \
         small fraction)",
        collisions.len()
    );

    // Acceptance case: the Verse.Verb_LaunchProjectile.TryCastShot collision
    // is documented, already-verified evidence on this install, independent
    // of kind extraction — its absence here would mean the mod set changed or
    // collision detection itself regressed. Whether it resolves to
    // two-or-more transpiler owners is what this measurement reports, so it's
    // never asserted true or false here.
    const ACCEPTANCE_TYPE: &str = "Verse.Verb_LaunchProjectile";
    const ACCEPTANCE_METHOD: &str = "TryCastShot";
    let acceptance_collision_present = collisions
        .iter()
        .any(|c| c.target_type == ACCEPTANCE_TYPE && c.target_method == ACCEPTANCE_METHOD);
    let acceptance_transpiler_owners = owners_declaring_kind(
        &scan,
        ACCEPTANCE_TYPE,
        ACCEPTANCE_METHOD,
        RuntimePatchKind::Transpiler,
    );

    eprintln!(
        "acceptance case: {ACCEPTANCE_TYPE}.{ACCEPTANCE_METHOD} collision present = \
         {acceptance_collision_present}, transpiler owners = {acceptance_transpiler_owners:?} \
         ({} of them)",
        acceptance_transpiler_owners.len()
    );
    assert!(
        acceptance_collision_present,
        "{ACCEPTANCE_TYPE}.{ACCEPTANCE_METHOD} is documented evidence \
         of an existing RuntimePatchCollision on this install — its absence here means either the \
         mod set changed or something regressed collision detection itself"
    );
}
