//! Validates the hand-rolled ECMA-335 reader against ground truth from
//! `System.Reflection.Metadata` (dumped via a small .NET console probe).
//!
//! Requires a real RimWorld install with a real assembly present, so it's
//! `#[ignore]`d by default and gated on `RIMMERGE_GAME_DIR` like the rest of
//! the real-install tier — unset, every test here skips with an honest
//! `skipping:` line rather than failing on a path that was hardcoded to one
//! machine.
//!
//! **No third-party mod is named here**: the DLL to read comes entirely from
//! `RIMMERGE_GROUND_TRUTH_DLL`/`RIMMERGE_GROUND_TRUTH_WORKSHOP_DLL` (absolute
//! paths, chosen by the maintainer running this tier), and the shape
//! assertions below (parses, non-empty name/references, a load-time reference
//! set that's a subset of the full one) hold for any assembly. A maintainer
//! who wants exact-value assertions sets the matching
//! `_NAME`/`_REFS`/`_VERSION`/`_RUNTIME_PATCH_TARGET` pin variables for their own
//! machine; unset, only the shape is checked.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> \
//! RIMMERGE_GROUND_TRUTH_DLL=<path to a real Mods/*/Assemblies/*.dll> cargo nextest run \
//! -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::HashSet;
use std::env;

use rim_analyzer::domain::AssemblyVersion;
use rim_analyzer::extract::pe_metadata;

mod common;

/// This file's own "Run with" invocation (the module doc comment above).
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> RIMMERGE_GROUND_TRUTH_DLL=<path to a real \
     Mods/*/Assemblies/*.dll> cargo nextest run -p rim-analyzer --all-features --release \
     --run-ignored ignored-only";

/// An optional exact pin, read from `{env_prefix}_NAME`/`_REFS`/`_VERSION`/
/// `_RUNTIME_PATCH_TARGET`. Every field is independently optional: a maintainer
/// pins only the values they know for their own DLL.
struct GroundTruthPin {
    name: Option<String>,
    refs: Option<Vec<String>>,
    version: Option<AssemblyVersion>,
    runtime_target: Option<(String, String)>,
}

fn parse_version(env_var: &str, value: &str) -> AssemblyVersion {
    let parts: Vec<u16> = value
        .split('.')
        .map(|part| {
            part.parse()
                .unwrap_or_else(|e| panic!("parsing {env_var}={value}: {e}"))
        })
        .collect();
    let [major, minor, build, revision] = parts[..] else {
        panic!("{env_var} must be major.minor.build.revision, got {value}");
    };
    AssemblyVersion {
        major,
        minor,
        build,
        revision,
    }
}

fn ground_truth_pin(env_prefix: &str) -> GroundTruthPin {
    let refs_var = format!("{env_prefix}_REFS");
    let version_var = format!("{env_prefix}_VERSION");
    let runtime_target_var = format!("{env_prefix}_RUNTIME_PATCH_TARGET");
    GroundTruthPin {
        name: env::var(format!("{env_prefix}_NAME")).ok(),
        refs: env::var(&refs_var)
            .ok()
            .map(|v| v.split(',').map(str::to_owned).collect()),
        version: env::var(&version_var)
            .ok()
            .map(|v| parse_version(&version_var, &v)),
        runtime_target: env::var(&runtime_target_var).ok().map(|v| {
            let (type_name, method_name) = v
                .split_once("::")
                .unwrap_or_else(|| panic!("{runtime_target_var} must be Type::Method, got {v}"));
            (type_name.to_owned(), method_name.to_owned())
        }),
    }
}

/// Reads and parses the DLL named by `{env_prefix}_DLL`, or `None` when
/// that variable is unset or this machine isn't in the real-install tier.
fn read_ground_truth_dll(
    env_prefix: &str,
) -> Option<(pe_metadata::AssemblyMetadata, GroundTruthPin)> {
    common::require_game_dir(RERUN_COMMAND)?;
    let dll_var = format!("{env_prefix}_DLL");
    let dll_path = common::require_pin_var(&dll_var, RERUN_COMMAND)?;
    let bytes = std::fs::read(&dll_path).unwrap_or_else(|e| panic!("reading {dll_path}: {e}"));
    let metadata = pe_metadata::read(&bytes).unwrap_or_else(|e| panic!("parsing {dll_path}: {e}"));
    Some((metadata, ground_truth_pin(env_prefix)))
}

/// Shape assertions that hold for any real assembly, regardless of which
/// one the maintainer pointed at — the part of this test that still runs
/// even when no exact-value pin is set.
fn assert_ground_truth_shape(metadata: &pe_metadata::AssemblyMetadata) {
    assert!(!metadata.name.is_empty(), "assembly name must not be empty");
    assert!(
        !metadata.references.is_empty(),
        "a real mod assembly should reference at least mscorlib/Assembly-CSharp"
    );

    let all_names: HashSet<&str> = metadata
        .references
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let load_time_names: HashSet<&str> = metadata
        .references
        .iter()
        .filter(|r| r.load_time)
        .map(|r| r.name.as_str())
        .collect();
    assert!(
        load_time_names.is_subset(&all_names),
        "load-time references must be a subset of every reference"
    );

    for patch in &metadata.runtime_patches {
        assert!(
            !patch.type_name.is_empty() && !patch.method_name.is_empty(),
            "a decoded runtime patch target must name both a type and a method, got {patch:?}"
        );
    }
}

/// Exact-value assertions, run only for the fields the maintainer pinned.
fn assert_ground_truth_pin(metadata: &pe_metadata::AssemblyMetadata, pin: &GroundTruthPin) {
    if let Some(expected_name) = &pin.name {
        assert_eq!(&metadata.name, expected_name, "assembly name mismatch");
    }
    if let Some(expected_refs) = &pin.refs {
        let mut actual: Vec<String> = metadata.references.iter().map(|r| r.name.clone()).collect();
        actual.sort();
        let mut expected = expected_refs.clone();
        expected.sort();
        assert_eq!(actual, expected, "reference set mismatch");
    }
    if let Some(expected_version) = &pin.version {
        assert_eq!(
            &metadata.version, expected_version,
            "assembly version mismatch"
        );
    }
    if let Some((expected_type, expected_method)) = &pin.runtime_target {
        assert!(
            metadata
                .runtime_patches
                .iter()
                .any(|p| &p.type_name == expected_type && &p.method_name == expected_method),
            "expected a {expected_type}::{expected_method} runtime patch target, got {:?}",
            metadata.runtime_patches
        );
    }
}

#[test]
#[ignore = "requires a real RimWorld install and RIMMERGE_GROUND_TRUTH_DLL on this machine"]
fn mods_dll_matches_system_reflection_metadata_ground_truth() {
    let Some((metadata, pin)) = read_ground_truth_dll("RIMMERGE_GROUND_TRUTH") else {
        return;
    };
    assert_ground_truth_shape(&metadata);
    assert_ground_truth_pin(&metadata, &pin);
}

#[test]
#[ignore = "requires a real RimWorld workshop subscription and RIMMERGE_GROUND_TRUTH_WORKSHOP_DLL \
            on this machine"]
fn workshop_dll_matches_system_reflection_metadata_ground_truth() {
    let Some((metadata, pin)) = read_ground_truth_dll("RIMMERGE_GROUND_TRUTH_WORKSHOP") else {
        return;
    };
    assert_ground_truth_shape(&metadata);
    assert_ground_truth_pin(&metadata, &pin);
}
