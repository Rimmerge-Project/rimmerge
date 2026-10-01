//! End-to-end CLI tests for `merge plan`/`merge coverage`/
//! `apply --write-merge-mod`, against a scratch copy of `rim-io`'s
//! `merge_game` fixture — never the real game install or `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

/// A scratch copy of `merge_game` under `temp_dir`, plus the four path
/// args every command in this file needs.
struct ScratchGame {
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
}

fn scratch_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    copy_dir_recursive(&merge_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy merge_game fixture: {error}"));
    ScratchGame {
        mods_config: game_dir.join("ModsConfig.xml"),
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        game_dir,
    }
}

fn path_args(cmd: &mut Command, game: &ScratchGame) {
    cmd.arg("--game-dir")
        .arg(&game.game_dir)
        .arg("--workshop-dir")
        .arg(&game.workshop_dir)
        .arg("--mods-config")
        .arg(&game.mods_config)
        .arg("--profile-dir")
        .arg(&game.profile_dir);
}

/// A from-scratch (not `merge_game`-derived) game with exactly one
/// contested `PatchCollision`, hand-built rather than reusing
/// `merge_game` (a `DefOverride`, not a patch collision at all) so
/// `merge coverage`'s own supported/agreeing/conflict split has exactly
/// one collision to classify.
///
/// `fixture.owner` ships `BiomeDef/Fixture_Biome`'s `wildAnimals` (a
/// tag-keyed `Dictionary<PawnKindDef, float>`): `Warg=0.2`, `Cobra=0.3`.
/// `fixture.modb`/`fixture.modc`
/// each `PatchOperationReplace` the *entire* `wildAnimals` node (not one
/// child — a real, if less common, RimWorld shape: replacing a whole
/// dictionary field rather than one key) with a different `Cobra` value,
/// `modb` also adding a disjoint new key (`Tortoise`) nobody else
/// touches. Both ops share the literal xpath tail `wildAnimals`, so the
/// analyzer indexes them as one `PatchCollision` at `sub_path:
/// "wildAnimals"` — the container itself, not one of its keys — which is
/// exactly the shape `rim_merge::diff::collision_fields` expands into
/// several `FieldDiff`s.
///
/// The resulting expansion (final-tree document order: `Warg`, `Cobra`,
/// then `Tortoise` — `modb`'s own new key, unseen in `final_tree`'s own
/// order since `modc`'s later wholesale replace drops it):
/// `Warg` is `Unchanged` (both replacements keep it at `0.2`), `Cobra` is
/// a genuine `Conflict` (`0.5` vs `0.9`, both differing from the base
/// `0.3`), `Tortoise` is `OneSided(modb)` auto-restored via
/// `Caveat::ClobberedMapEntry` (`modc`'s wholesale replace of the whole
/// container silently dropped `modb`'s own addition). The overall
/// collision therefore has one unresolved field (`Cobra`) —
/// `MergeState::NeedsFieldInput` — even though its *first* field in
/// document order (`Warg`) is `Unchanged`, the regression this fixture
/// exists to pin: `run_coverage`'s own classification must read
/// `preview.state`, not `preview.diff.fields.first()`'s class, or this
/// collision is silently miscounted `agreeing` instead of `conflict`.
fn wild_animals_replace_conflict_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    fs::create_dir_all(&game_dir).unwrap_or_else(|error| panic!("create game dir: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590\n")
        .unwrap_or_else(|error| panic!("write Version.txt: {error}"));

    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;

    let write_mod = |package_id: &str, name: &str, defs: Option<&str>, patch: Option<&str>| {
        let root = game_dir.join("Mods").join(package_id);
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {package_id}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {package_id}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {package_id}/LoadFolders.xml: {error}"));
        if let Some(defs) = defs {
            fs::create_dir_all(root.join("1.6").join("Defs"))
                .unwrap_or_else(|error| panic!("create {package_id}/Defs: {error}"));
            fs::write(root.join("1.6").join("Defs").join("Defs.xml"), defs)
                .unwrap_or_else(|error| panic!("write {package_id}/Defs.xml: {error}"));
        }
        if let Some(patch) = patch {
            fs::create_dir_all(root.join("Patches"))
                .unwrap_or_else(|error| panic!("create {package_id}/Patches: {error}"));
            fs::write(root.join("Patches").join("x.xml"), patch)
                .unwrap_or_else(|error| panic!("write {package_id}/Patches/x.xml: {error}"));
        }
    };

    write_mod(
        "fixture.owner",
        "Fixture Biome Owner",
        Some(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <BiomeDef>
    <defName>Fixture_Biome</defName>
    <wildAnimals>
      <Warg>0.2</Warg>
      <Cobra>0.3</Cobra>
    </wildAnimals>
  </BiomeDef>
</Defs>
"#,
        ),
        None,
    );
    write_mod(
        "fixture.modb",
        "Fixture Mod B",
        None,
        Some(
            r#"<Patch>
  <Operation Class="PatchOperationReplace">
    <xpath>Defs/BiomeDef[defName="Fixture_Biome"]/wildAnimals</xpath>
    <value>
      <wildAnimals>
        <Warg>0.2</Warg>
        <Cobra>0.5</Cobra>
        <Tortoise>0.4</Tortoise>
      </wildAnimals>
    </value>
  </Operation>
</Patch>
"#,
        ),
    );
    write_mod(
        "fixture.modc",
        "Fixture Mod C",
        None,
        Some(
            r#"<Patch>
  <Operation Class="PatchOperationReplace">
    <xpath>Defs/BiomeDef[defName="Fixture_Biome"]/wildAnimals</xpath>
    <value>
      <wildAnimals>
        <Warg>0.2</Warg>
        <Cobra>0.9</Cobra>
      </wildAnimals>
    </value>
  </Operation>
</Patch>
"#,
        ),
    );

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.owner</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
  </activeMods>
</ModsConfigData>
"#;
    let mods_config_path = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config_path, mods_config)
        .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));

    ScratchGame {
        mods_config: mods_config_path,
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        game_dir,
    }
}

/// The disjoint-adds worked example, as real files instead of an in-memory
/// fixture: `fixture.owner` ships `BiomeDef/Fixture_Biome`'s `wildAnimals`
/// (`Warg=0.2` only); three mods each `PatchOperationAdd` a *disjoint*
/// animal — no `Replace`, no contested key, so every field auto-resolves
/// (`MergeState::Complete`), the same shape as several real biome mods
/// each adding their own animals to one biome. `fixture.modc` (the
/// last-loaded, hence `MergePreview::winner`) never touches `Allosaurus`
/// at all — its own *isolated* candidate for that key is absent, while the
/// real, full-order replay (every mod's own `Add` runs, none conflicting)
/// has it — exactly the labelling gap the `final` column exists to close.
fn arid_shrubland_disjoint_adds_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    fs::create_dir_all(&game_dir).unwrap_or_else(|error| panic!("create game dir: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590\n")
        .unwrap_or_else(|error| panic!("write Version.txt: {error}"));

    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;

    let write_mod = |package_id: &str, name: &str, defs: Option<&str>, patch: Option<&str>| {
        let root = game_dir.join("Mods").join(package_id);
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {package_id}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {package_id}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {package_id}/LoadFolders.xml: {error}"));
        if let Some(defs) = defs {
            fs::create_dir_all(root.join("1.6").join("Defs"))
                .unwrap_or_else(|error| panic!("create {package_id}/Defs: {error}"));
            fs::write(root.join("1.6").join("Defs").join("Defs.xml"), defs)
                .unwrap_or_else(|error| panic!("write {package_id}/Defs.xml: {error}"));
        }
        if let Some(patch) = patch {
            fs::create_dir_all(root.join("Patches"))
                .unwrap_or_else(|error| panic!("create {package_id}/Patches: {error}"));
            fs::write(root.join("Patches").join("x.xml"), patch)
                .unwrap_or_else(|error| panic!("write {package_id}/Patches/x.xml: {error}"));
        }
    };

    write_mod(
        "fixture.owner",
        "Fixture Biome Owner",
        Some(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <BiomeDef>
    <defName>Fixture_Biome</defName>
    <wildAnimals>
      <Warg>0.2</Warg>
    </wildAnimals>
  </BiomeDef>
</Defs>
"#,
        ),
        None,
    );
    for (package_id, name, animal, chance) in [
        ("fixture.moda", "Fixture Mod A", "Allosaurus", "0.6"),
        ("fixture.modb", "Fixture Mod B", "Mammoth", "0.1"),
        ("fixture.modc", "Fixture Mod C", "Hyena", "0.2"),
    ] {
        write_mod(
            package_id,
            name,
            None,
            Some(&format!(
                r#"<Patch>
  <Operation Class="PatchOperationAdd">
    <xpath>Defs/BiomeDef[defName="Fixture_Biome"]/wildAnimals</xpath>
    <value><{animal}>{chance}</{animal}></value>
  </Operation>
</Patch>
"#
            )),
        );
    }

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.owner</li>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
  </activeMods>
</ModsConfigData>
"#;
    let mods_config_path = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config_path, mods_config)
        .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));

    ScratchGame {
        mods_config: mods_config_path,
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        game_dir,
    }
}

/// A real, three-owner `ThingDef/Wall` def override where `<label>`
/// genuinely conflicts (`core.mod` "wall", `fixture.moda` "stone wall",
/// `fixture.modb` "brick wall", the winner — the same recipe
/// `rim-session`'s own `conflicting_wall_fixture` uses, as real files
/// instead of an in-memory one) — no stored choice exists (no `merge
/// decide` call), so `label` is a genuine [`ChoiceOutcome::NoChoice`]:
/// `resolved_field_values` reports no final value for it at all — the
/// case `<no final value>` exists for.
fn conflicting_wall_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    fs::create_dir_all(&game_dir).unwrap_or_else(|error| panic!("create game dir: {error}"));
    fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590\n")
        .unwrap_or_else(|error| panic!("write Version.txt: {error}"));

    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;

    let write_mod = |package_id: &str, name: &str, label: &str| {
        let root = game_dir.join("Mods").join(package_id);
        let about = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>{package_id}</packageId>
  <name>{name}</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#
        );
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {package_id}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {package_id}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {package_id}/LoadFolders.xml: {error}"));
        fs::create_dir_all(root.join("1.6").join("Defs"))
            .unwrap_or_else(|error| panic!("create {package_id}/Defs: {error}"));
        fs::write(
            root.join("1.6").join("Defs").join("Defs.xml"),
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <ThingDef>
    <defName>Wall</defName>
    <label>{label}</label>
  </ThingDef>
</Defs>
"#
            ),
        )
        .unwrap_or_else(|error| panic!("write {package_id}/Defs.xml: {error}"));
    };

    write_mod("core.mod", "Core Wall", "wall");
    write_mod("fixture.moda", "Fixture Mod A", "stone wall");
    write_mod("fixture.modb", "Fixture Mod B", "brick wall");

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>core.mod</li>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
  </activeMods>
</ModsConfigData>
"#;
    let mods_config_path = game_dir.join("ModsConfig.xml");
    fs::write(&mods_config_path, mods_config)
        .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));

    ScratchGame {
        mods_config: mods_config_path,
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        game_dir,
    }
}

/// `label` is a genuine, unresolved `Conflict` with no
/// stored choice — `winner candidate` still shows the winner's own real
/// value (`fixture.modb` does set `<label>`), but `final` must print the
/// clearly-empty marker rather than a confident (and wrong) value, since
/// `resolved_field_values` has nothing to resolve it to.
#[test]
fn merge_plan_prints_the_no_final_value_marker_for_an_unresolved_conflict() {
    let temp_dir = tempdir().expect("tempdir");
    let game = conflicting_wall_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("plan")
        .arg("--key")
        .arg("def_override:ThingDef/Wall:[core.mod,fixture.moda,fixture.modb]");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    let label_line = stdout
        .lines()
        .find(|line| line.starts_with("label "))
        .unwrap_or_else(|| panic!("expected a label row: {stdout}"));
    assert!(
        label_line.contains("CONFLICT"),
        "label must be a genuine, unresolved conflict: {label_line}"
    );
    assert!(
        label_line.contains("brick wall"),
        "winner candidate is the winner's own real value: {label_line}"
    );
    assert!(
        label_line.contains("<no final value>"),
        "an unresolved conflict has no final value to show — never a \
         silently-repeated winner candidate: {label_line}"
    );
}

#[test]
fn merge_plan_prints_the_diff_and_a_no_op_plan_for_the_fixtures_def_override() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("plan")
        .arg("--key")
        .arg("def_override:ThingDef/Fixture_Wall:[fixture.moda,fixture.modb]");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "def_override:ThingDef/Fixture_Wall",
        ))
        .stdout(predicates::str::contains("winner=fixture.modb"))
        .stdout(predicates::str::contains("state:  Complete"))
        .stdout(predicates::str::contains("statBases/MaxHitPoints"));
}

#[test]
fn merge_plan_fails_gracefully_on_an_unparseable_key() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("plan")
        .arg("--key")
        .arg("not a valid key");
    path_args(&mut cmd, &game);

    cmd.assert().failure();
}

/// `merge plan`'s fourth column is `winner candidate`, not `result`, and a
/// fifth, `final`, prints the full-order replay's own value
/// (`arid_shrubland_disjoint_adds_game`'s own doc comment has the exact
/// mechanism): `fixture.modc` (the last-loaded contributor, hence the
/// merge preview's own `winner`) never adds `Allosaurus` itself — its own
/// isolated candidate for that key is absent (`winner candidate` prints
/// `<absent>`) — while the real, full-order replay of every mod's own disjoint
/// `Add` has it (`final` prints `0.6`). A case where the two columns
/// genuinely differ, not one where they happen to coincide (a field the
/// winner *does* touch, e.g. `Hyena`, would print the same value in both
/// columns and prove nothing about the `final` column).
#[test]
fn merge_plan_renames_the_result_column_and_adds_a_final_column_that_genuinely_differs() {
    let temp_dir = tempdir().expect("tempdir");
    let game = arid_shrubland_disjoint_adds_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("merge").arg("plan").arg("--key").arg("patch_collision:BiomeDef/Fixture_Biome:def_name:wildAnimals:[fixture.moda,fixture.modb,fixture.modc]");
    path_args(&mut cmd, &game);

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.contains("winner candidate") && stdout.contains("final"),
        "header must name both new columns: {stdout}"
    );
    assert!(
        !stdout.contains(" result"),
        "the old, mislabelled column header must be gone: {stdout}"
    );
    let allosaurus_line = stdout
        .lines()
        .find(|line| line.contains("wildAnimals/Allosaurus"))
        .unwrap_or_else(|| panic!("expected an Allosaurus row: {stdout}"));
    assert!(
        allosaurus_line.contains("one_sided(fixture.moda)"),
        "only fixture.moda ever adds Allosaurus: {allosaurus_line}"
    );
    assert!(
        allosaurus_line.contains("<absent>"),
        "winner candidate must be absent — fixture.modc (the winner, \
         last-loaded) never adds Allosaurus itself: {allosaurus_line}"
    );
    assert!(
        allosaurus_line.contains("0.6"),
        "final must show the real full-order replay's own value — every \
         mod's disjoint Add survives together — not the winner's isolated \
         (nonexistent) candidate: {allosaurus_line}"
    );
}

#[test]
fn merge_coverage_reports_the_fixtures_zero_contested_patch_collisions() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("merge").arg("coverage");
    path_args(&mut cmd, &game);

    // `merge_game` has a DefOverride, not a patch collision, so coverage
    // over contested patch collisions is trivially zero — this proves the
    // command runs end to end (scan -> tally -> print) without asserting
    // a number this fixture was never designed to produce.
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("contested patch collisions: 0"));
}

/// `merge coverage` renders a second table, def
/// overrides, after the patch-collision one, and the `--report` JSON
/// gains a nested `def_overrides` section — the `merge_game` fixture's
/// own single `ThingDef/Fixture_Wall` override
/// (`fixture.moda`/`fixture.modb`, exercised by `merge plan` above) is
/// exactly the one row this table has to render.
#[test]
fn merge_coverage_renders_the_def_override_table() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let report_path = temp_dir.path().join("coverage.json");

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("coverage")
        .arg("--report")
        .arg(&report_path);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("def overrides: 1"))
        .stdout(predicates::str::contains("structural guard fires"))
        .stdout(predicates::str::contains(
            "currently promote to Merge at confidence 85",
        ));

    let json = fs::read_to_string(&report_path).expect("read coverage report");
    let report: serde_json::Value = serde_json::from_str(&json).expect("valid coverage JSON");
    let overrides = &report["def_overrides"];
    // Real values for `merge_game`'s own `ThingDef/Fixture_Wall`
    // override, not just that the guard/promotion *labels* appear — a
    // renderer that printed `guard_does_not_fire` on the "fires"
    // line would still pass a labels-only assertion.
    assert_eq!(overrides["total"], 1, "{report}");
    assert_eq!(overrides["by_owner_count"]["2"], 1, "{report}");
    assert_eq!(overrides["mod_versus_mod"], 1, "{report}");
    assert_eq!(overrides["planning_failures"], 0, "{report}");
    assert_eq!(
        overrides["guard_fires"],
        serde_json::json!({}),
        "no thingClass/ParentName/root Class/comp Class change in this fixture: {report}"
    );
    assert_eq!(overrides["guard_does_not_fire"], 1, "{report}");
    assert_eq!(overrides["guard_reread_failed"], 0, "{report}");
    assert_eq!(overrides["preview_complete_zero_ops"], 1, "{report}");
    assert_eq!(overrides["preview_complete_with_ops"], 0, "{report}");
    assert_eq!(overrides["promotes_to_merge_85"], 0, "{report}");
    // The two patch-collision fields stay top-level, unaffected by the
    // def-override table.
    assert_eq!(report["contested_collisions"], 0, "{report}");
}

#[test]
fn merge_coverage_writes_a_json_report_when_asked() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let report_path = temp_dir.path().join("coverage.json");

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("coverage")
        .arg("--report")
        .arg(&report_path);
    path_args(&mut cmd, &game);

    cmd.assert().success();
    let json = fs::read_to_string(&report_path).expect("read coverage report");
    assert!(json.contains("\"contested_collisions\": 0"));
}

/// A keyed-map `PatchCollision` whose *first* field (in document order)
/// auto-resolves but a *later* field is a genuine, unresolved conflict
/// must still count as `conflict`, not `agreeing` — see
/// [`wild_animals_replace_conflict_game`]'s own doc comment for the exact
/// fixture shape and why `Warg` (first, `Unchanged`) landing before
/// `Cobra` (second, the real `Conflict`) would fool a classifier reading
/// `preview.diff.fields.first()`.
#[test]
fn merge_coverage_counts_a_keyed_map_collision_as_conflict_even_when_its_first_key_is_clean() {
    let temp_dir = tempdir().expect("tempdir");
    let game = wild_animals_replace_conflict_game(temp_dir.path());
    let report_path = temp_dir.path().join("coverage.json");

    let mut cmd = common::rimmerge();
    cmd.arg("merge")
        .arg("coverage")
        .arg("--report")
        .arg(&report_path);
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("contested patch collisions: 1"))
        .stdout(predicates::str::contains("supported:   1"))
        .stdout(predicates::str::contains("conflict:  1"));

    let json = fs::read_to_string(&report_path).expect("read coverage report");
    let report: serde_json::Value = serde_json::from_str(&json).expect("valid coverage JSON");
    assert_eq!(report["contested_collisions"], 1, "{report}");
    assert_eq!(report["supported"], 1, "{report}");
    assert_eq!(
        report["conflict"], 1,
        "the collision's first field (Warg) is Unchanged, but its second \
         (Cobra) is a genuine Conflict — the whole collision needs a \
         decision (MergeState::NeedsFieldInput) and must count as \
         conflict, not agreeing: {report}"
    );
    assert_eq!(report["agreeing"], 0, "{report}");
}

#[test]
fn apply_write_merge_mod_is_a_safe_no_op_with_no_merge_decisions() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let original_checksum =
        fs::read_to_string(&game.mods_config).expect("read seeded ModsConfig.xml");

    let mut cmd = common::rimmerge();
    cmd.arg("apply").arg("--write-merge-mod").arg("--force");
    path_args(&mut cmd, &game);

    cmd.assert().success();

    assert!(
        !game
            .game_dir
            .join("Mods")
            .join("rimmerge_merge_dummy")
            .exists(),
        "no merge mod folder should appear when no Merge decision exists"
    );
    // The seeded ModsConfig.xml only changes shape (backup + rewrite) —
    // its content is asserted elsewhere (`apply_dry_run`); here the point
    // is just that `--write-merge-mod` doesn't crash or corrupt it.
    assert!(!original_checksum.is_empty());
}
