//! Always-run counterpart to `pe_metadata_ground_truth.rs` (which reads
//! the real install and is gated on it): reads the committed
//! `.dll` blobs under `crates/rim-resolve/tests/fixtures/synthetic/assemblies/bin/`
//! directly through [`pe_metadata::read`] and asserts the exact
//! load-time/lazy and runtime-patch-kind classification
//! `../../rim-resolve/tests/fixtures/synthetic/assemblies/README.md`
//! documents for each — no RimWorld install needed, so this runs in every
//! `cargo nextest run --workspace`.
//!
//! These blobs are what the synthetic install generator copies into generated
//! mod folders so the golden fixture carries real
//! `AssemblyRef`/runtime-patch evidence instead of a zero-`Hard`-edge
//! fallback.

use std::path::{Path, PathBuf};

use rim_analyzer::domain::RuntimePatchKind;
use rim_analyzer::extract::pe_metadata;

fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rim-resolve/tests/fixtures/synthetic/assemblies/bin")
}

fn read(name: &str) -> pe_metadata::AssemblyMetadata {
    let path: PathBuf = bin_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{} not found ({error}) -- run scripts/build-synthetic-assemblies.ps1",
            path.display()
        )
    });
    pe_metadata::read(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn has_reference(metadata: &pe_metadata::AssemblyMetadata, name: &str) -> Option<bool> {
    metadata
        .references
        .iter()
        .find(|reference| reference.name == name)
        .map(|reference| reference.load_time)
}

/// Recursive -- `bin/versioned/Exports.dll` (the `AssemblyVersionPrecedence`
/// fixture) sits one directory deeper than every other blob.
fn walk_dlls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk_dlls(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "dll") {
            out.push(path);
        }
    }
}

#[test]
fn every_committed_blob_parses() {
    let dir = bin_dir();
    if !dir.is_dir() {
        panic!(
            "{} not found -- run scripts/build-synthetic-assemblies.ps1",
            dir.display()
        );
    }
    let mut dlls = Vec::new();
    walk_dlls(&dir, &mut dlls);
    assert_eq!(
        dlls.len(),
        19,
        "expected 19 committed synthetic assembly blobs (4 base + 12 runtime-patch pairs + \
         ExportsSolo/ExtendsSolo + versioned/Exports), found {}: {dlls:?} -- update this count \
         and README.md together if the fixture set changes",
        dlls.len()
    );
    for dll in &dlls {
        let bytes = std::fs::read(dll).unwrap_or_else(|e| panic!("reading {dll:?}: {e}"));
        pe_metadata::read(&bytes).unwrap_or_else(|e| panic!("parsing {dll:?}: {e}"));
    }
}

#[test]
fn exports_declares_its_own_name_and_no_synthetic_references() {
    let metadata = read("Exports.dll");
    assert_eq!(metadata.name, "exports");
    // The only reference is the compiler-inserted `mscorlib` (the .NET
    // Framework base class library) — no reference to another synthetic
    // fixture, since this is the assembly other fixtures reference, not
    // the other way around.
    assert_eq!(has_reference(&metadata, "exports"), None);
}

#[test]
fn extends_hard_references_exports_at_load_time() {
    let metadata = read("ExtendsHard.dll");
    assert_eq!(
        has_reference(&metadata, "exports"),
        Some(true),
        "TypeDef.Extends must classify the Exports reference as load-time (Hard)"
    );
}

#[test]
fn calls_soft_references_exports_lazily() {
    let metadata = read("CallsSoft.dll");
    assert_eq!(
        has_reference(&metadata, "exports"),
        Some(false),
        "a method-body-only reference must classify Exports as lazy (Soft)"
    );
}

/// `type_hierarchy` on a real, `csc`-compiled cross-assembly `Extends`
/// reference: `ExtendsHard.dll`'s own `DerivedWidget : BaseWidget` names
/// its base by a `TypeRef` (`BaseWidget` lives in a different assembly,
/// `Exports.dll`), so the resolved base must carry `Exports.dll`'s own
/// namespace (`Example.Exports.BaseWidget`), not `ExtendsHard.dll`'s own
/// (`Example.Extends.BaseWidget`) — `analysis::inheritance`'s subclass
/// check depends on this being the *target* type's own namespace.
#[test]
fn extends_hard_records_its_base_types_own_full_name() {
    let metadata = read("ExtendsHard.dll");
    let base = metadata
        .type_hierarchy
        .iter()
        .find(|(name, _)| name == "Example.Extends.DerivedWidget")
        .unwrap_or_else(|| panic!("DerivedWidget missing from {:?}", metadata.type_hierarchy));
    assert_eq!(base.1.as_deref(), Some("Example.Exports.BaseWidget"));
}

/// The exported base type itself has no declared base in source
/// (`class BaseWidget` with no `: X`), so `csc` emits `System.Object` as
/// its own `Extends` target — a real `TypeRef` this reader resolves to a
/// name, never treated as "no base at all" (`Extends == 0`, which only a
/// literal `TypeDefOrRef` value of `0` — an interface — produces).
#[test]
fn exports_own_base_widget_extends_system_object() {
    let metadata = read("Exports.dll");
    let base = metadata
        .type_hierarchy
        .iter()
        .find(|(name, _)| name == "Example.Exports.BaseWidget")
        .unwrap_or_else(|| panic!("BaseWidget missing from {:?}", metadata.type_hierarchy));
    assert_eq!(base.1.as_deref(), Some("System.Object"));
}

/// The two Transpiler-vs-Transpiler target pairs: both owners decode the
/// same `(type, method)` target and the `Transpiler` kind.
#[test]
fn transpiler_pairs_decode_the_same_target_and_kind() {
    for (a, b, target_type, target_method) in [
        (
            "SynthPatch01.dll",
            "SynthPatch02.dll",
            "Example.Patches.TargetAlpha",
            "DoAlpha",
        ),
        (
            "SynthPatch03.dll",
            "SynthPatch04.dll",
            "Example.Patches.TargetBeta",
            "DoBeta",
        ),
    ] {
        for name in [a, b] {
            let metadata = read(name);
            assert_eq!(metadata.runtime_patches.len(), 1, "{name}");
            let target = &metadata.runtime_patches[0];
            assert_eq!(target.type_name, target_type, "{name}");
            assert_eq!(target.method_name, target_method, "{name}");
            assert_eq!(target.kind, RuntimePatchKind::Transpiler, "{name}");
        }
    }
}

/// The four Prefix-vs-Postfix target pairs: same target, disjoint kinds
/// (neither is `Transpiler`).
#[test]
fn prefix_postfix_pairs_decode_the_same_target_and_disjoint_kinds() {
    for (prefix, postfix, target_type, target_method) in [
        (
            "SynthPatch05.dll",
            "SynthPatch06.dll",
            "Example.Patches.TargetGamma",
            "DoGamma",
        ),
        (
            "SynthPatch07.dll",
            "SynthPatch08.dll",
            "Example.Patches.TargetDelta",
            "DoDelta",
        ),
        (
            "SynthPatch09.dll",
            "SynthPatch10.dll",
            "Example.Patches.TargetEpsilon",
            "DoEpsilon",
        ),
        (
            "SynthPatch11.dll",
            "SynthPatch12.dll",
            "Example.Patches.TargetZeta",
            "DoZeta",
        ),
    ] {
        let prefix_metadata = read(prefix);
        let postfix_metadata = read(postfix);
        for (metadata, name) in [(&prefix_metadata, prefix), (&postfix_metadata, postfix)] {
            assert_eq!(metadata.runtime_patches.len(), 1, "{name}");
            let target = &metadata.runtime_patches[0];
            assert_eq!(target.type_name, target_type, "{name}");
            assert_eq!(target.method_name, target_method, "{name}");
        }
        assert_eq!(
            prefix_metadata.runtime_patches[0].kind,
            RuntimePatchKind::Prefix
        );
        assert_eq!(
            postfix_metadata.runtime_patches[0].kind,
            RuntimePatchKind::Postfix
        );
    }
}

/// A duplicate-name copy needs no separate source (`README.md`): the
/// generator copies `Exports.dll`'s own bytes verbatim into a second mod
/// folder, so the copy this test reads is byte-identical and must decode
/// to the exact same assembly name.
#[test]
fn a_byte_identical_copy_of_exports_decodes_to_the_same_assembly_name() {
    let original = bin_dir().join("Exports.dll");
    let copy_dir = std::env::temp_dir().join(format!(
        "rim-analyzer-synthetic-assemblies-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&copy_dir).expect("create scratch dir");
    let copy_path: PathBuf = copy_dir.join("Exports.dll");
    std::fs::copy(&original, &copy_path).expect("copy Exports.dll");

    let original_metadata = pe_metadata::read(&std::fs::read(&original).unwrap()).unwrap();
    let copy_metadata = pe_metadata::read(&std::fs::read(&copy_path).unwrap()).unwrap();
    assert_eq!(original_metadata.name, copy_metadata.name);

    let _ = std::fs::remove_dir_all(&copy_dir);
}

/// The unambiguous single-owner Hard chain: `ExtendsSolo.dll` extends
/// `ExportsSolo.dll`'s own type, at load time, and `ExportsSolo` is never
/// duplicated (unlike `Exports`), so this reference is never ambiguous in the
/// generated install.
#[test]
fn extends_solo_references_exports_solo_at_load_time() {
    let metadata = read("ExtendsSolo.dll");
    assert_eq!(
        has_reference(&metadata, "exportssolo"),
        Some(true),
        "TypeDef.Extends must classify the ExportsSolo reference as load-time (Hard)"
    );
}

/// `AssemblyVersionPrecedence` needs two owners with real, distinct, non-zero
/// versions -- `bin/Exports.dll` (1.0.0.0) and `bin/versioned/Exports.dll`
/// (1.1.0.0) are the same `Assembly` table name ("exports") at two different
/// versions, both explicit (neither the CLR's own `0.0.0.0` default, which
/// the edge producer drops from consideration entirely -- see
/// `ExportsVersioned.cs`'s doc comment).
#[test]
fn versioned_exports_shares_the_plain_copys_name_at_a_higher_version() {
    let plain = read("Exports.dll");
    let versioned_path = bin_dir().join("versioned").join("Exports.dll");
    let versioned_bytes = std::fs::read(&versioned_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", versioned_path.display()));
    let versioned = pe_metadata::read(&versioned_bytes)
        .unwrap_or_else(|e| panic!("parsing {}: {e}", versioned_path.display()));

    assert_eq!(plain.name, "exports");
    assert_eq!(versioned.name, "exports");
    assert_eq!(
        plain.version,
        rim_analyzer::domain::AssemblyVersion {
            major: 1,
            minor: 0,
            build: 0,
            revision: 0
        }
    );
    assert_eq!(
        versioned.version,
        rim_analyzer::domain::AssemblyVersion {
            major: 1,
            minor: 1,
            build: 0,
            revision: 0
        }
    );
    assert!(versioned.version > plain.version);
}

/// Sanity: nothing here accidentally depends on this crate's own source
/// tree layout being anything other than a sibling of `rim-resolve`.
#[test]
fn bin_dir_resolves_under_rim_resolve() {
    let dir = bin_dir();
    assert!(
        dir.components().any(|c| c.as_os_str() == "rim-resolve"),
        "{}",
        dir.display()
    );
    assert!(Path::new(&dir).is_dir(), "{}", dir.display());
}
