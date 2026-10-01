//! Tests for the synthetic install generator.

use super::spec::{Archetypes, Planted};
use super::{Spec, generate};
use std::path::PathBuf;

/// The committed spec's own shape, reproduced by hand rather than
/// read from disk -- these tests exercise `generate`'s own
/// pre-filesystem validation, which never reaches the committed
/// spec's real assembly blobs, so this needn't depend on that file.
fn valid_spec() -> Spec {
    Spec {
        schema: 1,
        seed: 1,
        game_version: "1.6".to_string(),
        // frameworks(2) + framework_dependents(2) + content_mods(2) +
        // retextures(0) + translations(0) + patch_only(13, the xpath
        // zoo's own floor) = 19.
        mod_count: 19,
        archetypes: Archetypes {
            frameworks: 2,
            framework_dependents: 2,
            content_mods: 2,
            retextures: 0,
            translations: 0,
            patch_only: 13,
        },
        planted: Planted {
            def_overrides: 0,
            patch_collisions: 0,
            texture_overrides: 0,
            sound_overrides: 0,
            duplicate_assemblies: 3,
            likely_duplicate_mods: 0,
            duplicate_template_names: 0,
            keyed_translation_collisions: 0,
            missing_texture_paths: 0,
            runtime_patch_collisions: 6,
            transpiler_collisions: 2,
            cycles_forcing_dropped_edges: 0,
            any_of_constraints: 6,
            missing_dependencies: 0,
            incompatible_pairs: 0,
            unsupported_versions: 0,
            top_pinned: 0,
            bottom_pinned: 0,
        },
        inactive: 0,
    }
}

/// A syntactically valid but nonexistent path -- fine for the
/// validation-only failure cases below, which `generate` rejects
/// before ever touching the filesystem.
fn dummy_paths() -> (PathBuf, PathBuf) {
    (PathBuf::from("spec.json"), PathBuf::from("out"))
}

#[test]
fn rejects_an_unsupported_schema_version() {
    let mut spec = valid_spec();
    spec.schema = 2;
    let (spec_path, out) = dummy_paths();
    let error = generate(&spec, &spec_path, &out).expect_err("schema 2 must be rejected");
    assert!(error.to_string().contains("schema"), "{error}");
}

#[test]
fn rejects_a_game_version_other_than_1_6() {
    let mut spec = valid_spec();
    spec.game_version = "1.5".to_string();
    let (spec_path, out) = dummy_paths();
    let error = generate(&spec, &spec_path, &out).expect_err("game_version 1.5 must be rejected");
    assert!(error.to_string().contains("game_version"), "{error}");
}

#[test]
fn rejects_an_archetype_sum_that_disagrees_with_mod_count() {
    let mut spec = valid_spec();
    spec.mod_count += 1;
    let (spec_path, out) = dummy_paths();
    let error =
        generate(&spec, &spec_path, &out).expect_err("a mismatched mod_count must be rejected");
    assert!(error.to_string().contains("mod_count"), "{error}");
}

#[test]
fn rejects_too_few_patch_only_mods_for_the_zoos_own_one_off_constructs() {
    let mut spec = valid_spec();
    spec.archetypes.patch_only = 3;
    spec.mod_count = spec.archetypes.frameworks
        + spec.archetypes.framework_dependents
        + spec.archetypes.content_mods
        + spec.archetypes.retextures
        + spec.archetypes.translations
        + spec.archetypes.patch_only;
    let (spec_path, out) = dummy_paths();
    let error = generate(&spec, &spec_path, &out)
        .expect_err("too few patch_only mods for the zoo's own indices must be rejected");
    assert!(error.to_string().contains("patch_only"), "{error}");
}

/// `(mutator, the error text it must produce)` -- one case per
/// assembly-derived `planted` field
/// `rejects_planted_counts_that_disagree_with_the_fixed_assembly_set`
/// checks. These are rejected by `validate_spec`, before `generate`
/// ever touches the filesystem, so (unlike the real-blob-dependent
/// test below) a dummy nonexistent path is fine here too.
type MismatchCase = (fn(&mut Spec), &'static str);

#[test]
fn rejects_planted_counts_that_disagree_with_the_fixed_assembly_set() {
    let cases: Vec<MismatchCase> = vec![
        (
            (|s: &mut Spec| s.planted.duplicate_assemblies = 1) as fn(&mut Spec),
            "duplicate_assemblies",
        ),
        (
            (|s: &mut Spec| s.planted.any_of_constraints = 1) as fn(&mut Spec),
            "any_of_constraints",
        ),
        (
            (|s: &mut Spec| s.planted.runtime_patch_collisions = 1) as fn(&mut Spec),
            "runtime_patch_collisions",
        ),
        (
            (|s: &mut Spec| s.planted.transpiler_collisions = 1) as fn(&mut Spec),
            "transpiler_collisions",
        ),
    ];
    for (mutate, needle) in cases {
        let mut spec = valid_spec();
        mutate(&mut spec);
        let (spec_path, out) = dummy_paths();
        let error = generate(&spec, &spec_path, &out)
            .expect_err("a mismatched assembly-derived planted count must be rejected");
        assert!(error.to_string().contains(needle), "{error}");
    }
}

#[test]
fn rejects_a_nonempty_out_directory() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("creating tempdir: {e}"));
    std::fs::write(dir.path().join("existing.txt"), b"hello")
        .unwrap_or_else(|e| panic!("seeding tempdir: {e}"));
    let (spec_path, _) = dummy_paths();
    let error = generate(&valid_spec(), &spec_path, dir.path())
        .expect_err("a nonempty --out must be rejected");
    assert!(error.to_string().contains("must be empty"), "{error}");
}

/// The real committed spec's own directory, for the one test below
/// that needs the real committed assembly blobs (a mismatched
/// assembly-derived `planted` field could equally be caught earlier
/// by `validate_spec`, which runs every check up front, but
/// this crate's own generation still needs to succeed on a real spec
/// against a real, writable directory at least once).
fn real_spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-resolve")
        .join("tests")
        .join("fixtures")
        .join("synthetic-install.json")
}

#[test]
fn generates_successfully_against_the_real_committed_spec_and_assemblies() {
    let bytes = std::fs::read(real_spec_path())
        .unwrap_or_else(|e| panic!("reading the committed spec: {e}"));
    let spec: Spec = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parsing the committed spec: {e}"));
    let out = tempfile::tempdir().unwrap_or_else(|e| panic!("creating tempdir: {e}"));
    let summary = generate(&spec, &real_spec_path(), out.path())
        .unwrap_or_else(|e| panic!("generate failed against the real committed spec: {e}"));
    // The archetype mods alone (`spec.mod_count`) are a strict lower
    // bound -- every `planted` construct implemented as its own
    // dedicated extra mod (cycles, incompatible pairs, ...) adds more
    // on top.
    assert!(summary.active_mods >= spec.mod_count);
    assert!(out.path().join("game").join("ModsConfig.xml").is_file());
}
