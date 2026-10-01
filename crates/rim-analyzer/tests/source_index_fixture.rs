//! `SourceIndex`'s inverse maps — `owners_by_def`, `ops_by_mod`,
//! `children_by_template`, and
//! `defs_by_name` — must agree
//! with the forward maps (`defs`, `patch_ops_by_def`, `templates`) they're
//! built from, over the same real scan `fixture_scan.rs` already
//! exercises — not just the hand-built maps in `source_index`'s own unit
//! tests.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rim_analyzer::domain::{FolderPolicy, GameVersion, ModId, Selector};
use rim_analyzer::{analysis, infra};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_game")
}

fn scan_config() -> infra::ScanConfig {
    let game_dir = fixture_dir();
    infra::ScanConfig {
        mods_config_path: game_dir.join("ModsConfig.xml"),
        workshop_dir: game_dir.join("workshop_does_not_exist"),
        game_dir,
        game_version: GameVersion::new(1, 6),
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: None,
    }
}

/// Every key `defs` knows about appears in `owners_by_def` naming the same
/// owners, and vice versa — the fixture's one mod, one def case is the
/// simplest instance of the general invariant.
#[test]
fn owners_by_def_agrees_with_defs_on_the_fixture() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let index = analysis::source_index::build(&output);

    let def_keys_from_defs: BTreeSet<_> = index.defs.keys().map(|(_, key)| key.clone()).collect();
    let def_keys_from_owners: BTreeSet<_> = index.owners_by_def.keys().cloned().collect();
    assert_eq!(def_keys_from_defs, def_keys_from_owners);
    assert!(!def_keys_from_defs.is_empty(), "the fixture defines a def");

    for (def_key, owners) in &index.owners_by_def {
        for owner in owners {
            assert!(
                index.defs.contains_key(&(owner.clone(), def_key.clone())),
                "owners_by_def names {owner:?} for {def_key:?} but defs has no matching entry"
            );
        }
    }
}

/// `defs_by_name` agrees with `owners_by_def` on the fixture: every def's
/// name lists its `def_type` and owner, and the owners for one name match
/// `owners_by_def` for the corresponding `(def_type, def_name)` key exactly —
/// the name index over a real scan, not just the hand-built maps in
/// `source_index`'s own unit tests.
#[test]
fn defs_by_name_agrees_with_owners_by_def_on_the_fixture() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let index = analysis::source_index::build(&output);

    assert!(
        !index.defs_by_name.is_empty(),
        "the fixture defines at least one def"
    );

    for (def_type, def_name) in index.defs.keys().map(|(_, key)| key.clone()) {
        let by_name = index
            .defs_by_name
            .get(&def_name)
            .unwrap_or_else(|| panic!("{def_name} must be indexed in defs_by_name"));
        assert!(
            by_name.iter().any(|(ty, _)| *ty == def_type),
            "defs_by_name[{def_name}] is missing def_type {def_type}"
        );

        let owners_from_name: BTreeSet<_> = by_name
            .iter()
            .filter(|(ty, _)| *ty == def_type)
            .map(|(_, owner)| owner.clone())
            .collect();
        let owners_from_owners_by_def: BTreeSet<_> = index.owners_by_def
            [&(def_type.clone(), def_name.clone())]
            .iter()
            .cloned()
            .collect();
        assert_eq!(owners_from_name, owners_from_owners_by_def);
    }

    // Reverse direction: every `defs_by_name` entry maps back to a real
    // `owners_by_def` entry naming the same owner for the same type — the
    // loop above only walks `defs`/`owners_by_def` outward into
    // `defs_by_name`, so on its own it can't catch a spurious extra entry
    // `defs_by_name` invents that `owners_by_def` never agreed to (a
    // leaked template `Name`, for instance).
    for (def_name, owners) in &index.defs_by_name {
        for (def_type, owner) in owners {
            let key = (def_type.clone(), def_name.clone());
            let known_owners = index.owners_by_def.get(&key).unwrap_or_else(|| {
                panic!("defs_by_name names {key:?} but owners_by_def has no entry for it")
            });
            assert!(
                known_owners.contains(owner),
                "defs_by_name says {owner:?} owns {key:?}, but owners_by_def disagrees"
            );
        }
    }
}

/// `ops_by_mod`'s totals equal the number of distinct top-level
/// `<Operation>` ancestors touching each key — never the flattened
/// `patch_ops_by_def` total, and never limited to ops whose own
/// `element_path.len() == 1` (that predicate is not the dedup rule; see
/// `IndexedPatchOp::is_top_level`'s own doc comment), which would drop the
/// fixture's nested `PatchOperationFindMod` descendant instead of counting
/// its own top-level ancestor.
#[test]
fn ops_by_mod_totals_equal_top_level_ancestor_counts_on_the_fixture() {
    let output = infra::scan(&scan_config()).expect("fixture scan must succeed");
    let index = analysis::source_index::build(&output);

    let actual_total: usize = index
        .ops_by_mod
        .values()
        .flat_map(std::collections::BTreeMap::values)
        .sum();

    // The fixture's own known shape (see `fixture_scan.rs`): two top-level
    // leaves in `AAA_Earlier.xml` (`statBases`, `tradeTags`), plus a third
    // mutating leaf nested two `PatchOperationFindMod` branches deep in
    // `Patch.xml` (`comps`) — all three target `SampleWall`, so this one
    // key's count is three distinct top-level ancestors, not the two a
    // length-1-`element_path` filter would report.
    assert_eq!(actual_total, 3);
    let key = (
        "ThingDef".to_string(),
        "SampleWall".to_string(),
        Selector::DefName,
    );
    assert_eq!(index.patch_ops_by_def[&key].len(), 3);
    assert_eq!(
        index.ops_by_mod[&ModId::new("sample.mod")].get(&key),
        Some(&3)
    );
}
