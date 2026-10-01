//! End-to-end CLI tests for `defs changes`/`defs inspect`/`defs search`,
//! against a scratch copy of `rim-io`'s `merge_game` fixture — never the
//! real game install or `ModsConfig.xml`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;
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

/// Adds a third mod to the scratch game that also defines
/// `ThingDef/Fixture_Wall`, with **no** `ParentName` of its own, and
/// activates it last.
///
/// Written into the temp copy rather than into the shared
/// `crates/rim-io/tests/fixtures/merge_game` tree, which a dozen other
/// tests read: this is the one test that needs a third owner.
///
/// Exists so `--source suggested` still has a case where the suggested
/// order genuinely differs from the current one. ModA and ModB cannot be
/// flipped by a rule — ModB inherits a template only ModA registers,
/// which is a `Hard` edge — but ModC is under no such constraint, so a
/// pair rule pulling it ahead of ModB moves the winner from ModC to ModB.
fn add_unconstrained_third_owner(game: &ScratchGame) {
    let mod_dir = game.game_dir.join("Mods").join("ModC");
    fs::create_dir_all(mod_dir.join("About"))
        .unwrap_or_else(|error| panic!("create ModC/About: {error}"));
    fs::create_dir_all(mod_dir.join("1.6").join("Defs"))
        .unwrap_or_else(|error| panic!("create ModC/1.6/Defs: {error}"));
    fs::write(
        mod_dir.join("About").join("About.xml"),
        r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modc</packageId>
  <name>Fixture Mod C</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#,
    )
    .unwrap_or_else(|error| panic!("write ModC About.xml: {error}"));
    fs::write(
        mod_dir.join("LoadFolders.xml"),
        "<loadFolders>\n  <v1.6>\n    <li>1.6</li>\n    <li>/</li>\n  </v1.6>\n</loadFolders>\n",
    )
    .unwrap_or_else(|error| panic!("write ModC LoadFolders.xml: {error}"));
    fs::write(
        mod_dir.join("1.6").join("Defs").join("ThingDefs.xml"),
        r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <ThingDef>
    <defName>Fixture_Wall</defName>
    <label>third fixture wall</label>
  </ThingDef>
</Defs>
"#,
    )
    .unwrap_or_else(|error| panic!("write ModC ThingDefs.xml: {error}"));

    let config = fs::read_to_string(&game.mods_config)
        .unwrap_or_else(|error| panic!("read ModsConfig.xml: {error}"));
    let config = config.replace("</activeMods>", "  <li>fixture.modc</li>\n  </activeMods>");
    fs::write(&game.mods_config, config)
        .unwrap_or_else(|error| panic!("write ModsConfig.xml: {error}"));
}

/// Writes a `rules.json` with one pair rule pulling `fixture.modc` ahead
/// of `fixture.modb` — a pair no derived edge constrains, so the sorter
/// honours it and the suggested order really does differ from the current
/// one. See [`add_unconstrained_third_owner`].
fn write_third_owner_pair_rule(game: &ScratchGame) {
    fs::create_dir_all(&game.profile_dir)
        .unwrap_or_else(|error| panic!("create profile dir: {error}"));
    let rules = r#"{
  "version": 2,
  "pairs": [
    { "after": "fixture.modb", "before": "fixture.modc", "origin": "user_decision", "comment": null }
  ],
  "placements": [],
  "incompatibles": [],
  "tag_rules": [],
  "manual_tags": [],
  "settings": {
    "threshold": 80,
    "enforce_soft": false,
    "enforce_awareness": false,
    "suggest_merge_when_clean": true,
    "tie_break": "rebuild",
    "use_imported_pairs": false,
    "use_imported_placements": true
  }
}
"#;
    fs::write(game.profile_dir.join("rules.json"), rules)
        .unwrap_or_else(|error| panic!("write rules.json: {error}"));
}

/// Writes a `rules.json` with one pair rule pinning `fixture.modb` before
/// `fixture.moda`.
///
/// **The rule flips nothing, and that is the point**:
/// `ModB`'s own `Fixture_Wall` declares `ParentName="Fixture_WallBase2"`,
/// a template only `ModA` registers, so `Verse.XmlInheritance` drops
/// ModB's def outright if ModB loads first. That is an
/// `EdgeKind::ParentTemplate` edge at `EdgeStrength::Hard`, and
/// `Layer::Hard` is added before every rule layer — a user pair rule
/// asking for the opposite order is asking for the def to be deleted, and
/// loses. The template inheritance alone forces the order; the fixture
/// declares no dependency between the two.
fn write_flipping_pair_rule(game: &ScratchGame) {
    fs::create_dir_all(&game.profile_dir)
        .unwrap_or_else(|error| panic!("create profile dir: {error}"));
    let rules = r#"{
  "version": 2,
  "pairs": [
    { "after": "fixture.moda", "before": "fixture.modb", "origin": "user_decision", "comment": null }
  ],
  "placements": [],
  "incompatibles": [],
  "tag_rules": [],
  "manual_tags": [],
  "settings": {
    "threshold": 80,
    "enforce_soft": false,
    "enforce_awareness": false,
    "suggest_merge_when_clean": true,
    "tie_break": "rebuild",
    "use_imported_pairs": false,
    "use_imported_placements": true
  }
}
"#;
    fs::write(game.profile_dir.join("rules.json"), rules)
        .unwrap_or_else(|error| panic!("write rules.json: {error}"));
}

/// Extends `game`'s own scratch copy with two more active mods patching
/// `ThingDef/Fixture_Wall`, neither present in the checked-in fixture
/// itself (extending the *scratch* copy, never the shared fixture other
/// tests/crates also read): `fixture.modc` ships one top-level operation
/// of an unrecognised `Class` — `patch_eval::apply_mutation`'s own
/// catch-all falls through to `ReplayError::Unsupported` for any class it
/// doesn't recognise (see `crates/rim-merge/src/patch_eval.rs`) — which
/// stops `DefInspection.effective`'s fold; `fixture.modd` loads after it
/// with one operation wrapped inside a `PatchOperationSequence` (so its
/// own `PatchOpSummary::is_wrapped` is `true`) that the fold never
/// reaches. Both mods otherwise contribute nothing else to the fixture.
fn add_unsupported_patcher_and_a_later_patcher(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;

    let mod_c_about = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modc</packageId>
  <name>Fixture Mod C</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#;
    let mod_c_patch = r#"<Patch>
  <Operation Class="PatchOperationUnrecognisedFixture">
    <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
    <value>
      <label>never applied</label>
    </value>
  </Operation>
</Patch>
"#;

    let mod_d_about = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>fixture.modd</packageId>
  <name>Fixture Mod D</name>
  <author>Fixture Author</author>
  <supportedVersions>
    <li>1.6</li>
  </supportedVersions>
</ModMetaData>
"#;
    let mod_d_patch = r#"<Patch>
  <Operation Class="PatchOperationSequence">
    <operations>
      <li Class="PatchOperationReplace">
        <xpath>Defs/ThingDef[defName="Fixture_Wall"]/label</xpath>
        <value>
          <label>never reached</label>
        </value>
      </li>
    </operations>
  </Operation>
</Patch>
"#;

    for (mod_dir, about, patch) in [
        ("ModC", mod_c_about, mod_c_patch),
        ("ModD", mod_d_about, mod_d_patch),
    ] {
        let root = game.game_dir.join("Mods").join(mod_dir);
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {mod_dir}/About: {error}"));
        fs::write(root.join("About").join("About.xml"), about)
            .unwrap_or_else(|error| panic!("write {mod_dir}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {mod_dir}/LoadFolders.xml: {error}"));
        fs::create_dir_all(root.join("Patches"))
            .unwrap_or_else(|error| panic!("create {mod_dir}/Patches: {error}"));
        fs::write(root.join("Patches").join("x.xml"), patch)
            .unwrap_or_else(|error| panic!("write {mod_dir}/Patches/x.xml: {error}"));
    }

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.moda</li>
    <li>fixture.modb</li>
    <li>fixture.modc</li>
    <li>fixture.modd</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

/// Extends `game`'s own scratch copy with four more active mods, in load
/// order `modx, modchilda, mody, modchildb`, that together make
/// `ThingDef/@Fixture_AmbiguousBase` a genuinely ambiguous template: both
/// `fixture.modx` and `fixture.mody` register the same `Name`
/// (`Verse.XmlInheritance.TryRegister` allows this across mods — only a
/// duplicate *within* one mod errors), straddling `fixture.modchilda`'s
/// own real child of it. `fixture.modchildb` loads after both
/// registrants. Per the real `GetBestParentFor` rule (nearest registrant
/// at or before the child's own load order), `modchilda`'s child must
/// resolve to `modx` (the only registrant loaded before it) and
/// `modchildb`'s child must resolve to `mody` (the nearer of the two,
/// both loaded before it) — proving real per-child divergence, not just
/// that a "last-loaded" or "first-loaded" single answer happens to agree.
fn add_two_registrants_for_an_ambiguous_template(game: &ScratchGame) {
    let load_folders = r#"<loadFolders>
  <v1.6>
    <li>1.6</li>
    <li>/</li>
  </v1.6>
</loadFolders>
"#;

    let about_xml = |package_id: &str, name: &str| {
        format!(
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
        )
    };

    let registrant_defs = |max_hit_points: u32| {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <ThingDef Name="Fixture_AmbiguousBase" Abstract="True">
    <category>Building</category>
    <statBases>
      <MaxHitPoints>{max_hit_points}</MaxHitPoints>
    </statBases>
  </ThingDef>
</Defs>
"#
        )
    };

    let child_defs = |def_name: &str, label: &str| {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Defs>
  <ThingDef ParentName="Fixture_AmbiguousBase">
    <defName>{def_name}</defName>
    <label>{label}</label>
  </ThingDef>
</Defs>
"#
        )
    };

    let mods = [
        ("ModX", "fixture.modx", "Fixture Mod X", registrant_defs(10)),
        (
            "ModChildA",
            "fixture.modchilda",
            "Fixture Mod Child A",
            child_defs("Fixture_AmbiguousChildEarly", "early child"),
        ),
        ("ModY", "fixture.mody", "Fixture Mod Y", registrant_defs(20)),
        (
            "ModChildB",
            "fixture.modchildb",
            "Fixture Mod Child B",
            child_defs("Fixture_AmbiguousChildLate", "late child"),
        ),
    ];

    for (mod_dir, package_id, name, defs) in &mods {
        let root = game.game_dir.join("Mods").join(mod_dir);
        fs::create_dir_all(root.join("About"))
            .unwrap_or_else(|error| panic!("create {mod_dir}/About: {error}"));
        fs::write(
            root.join("About").join("About.xml"),
            about_xml(package_id, name),
        )
        .unwrap_or_else(|error| panic!("write {mod_dir}/About.xml: {error}"));
        fs::write(root.join("LoadFolders.xml"), load_folders)
            .unwrap_or_else(|error| panic!("write {mod_dir}/LoadFolders.xml: {error}"));
        fs::create_dir_all(root.join("1.6").join("Defs"))
            .unwrap_or_else(|error| panic!("create {mod_dir}/1.6/Defs: {error}"));
        fs::write(root.join("1.6").join("Defs").join("ThingDefs.xml"), defs)
            .unwrap_or_else(|error| panic!("write {mod_dir}/1.6/Defs/ThingDefs.xml: {error}"));
    }

    let mods_config = r#"<?xml version="1.0" encoding="utf-8"?>
<ModsConfigData>
  <version>1.6.4871 rev590</version>
  <activeMods>
    <li>fixture.modx</li>
    <li>fixture.modchilda</li>
    <li>fixture.mody</li>
    <li>fixture.modchildb</li>
  </activeMods>
</ModsConfigData>
"#;
    fs::write(&game.mods_config, mods_config)
        .unwrap_or_else(|error| panic!("rewrite ModsConfig.xml: {error}"));
}

fn run_json(game: &ScratchGame, args: &[&str]) -> Value {
    let mut cmd = common::rimmerge();
    cmd.arg("defs");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg("--json");
    path_args(&mut cmd, game);
    let output = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap_or_else(|error| {
        panic!(
            "expected valid JSON, got error {error} for output:\n{}",
            String::from_utf8_lossy(&output)
        )
    })
}

#[test]
fn defs_changes_lists_the_owned_and_contested_def() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("changes").arg("fixture.moda");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("ThingDef/Fixture_Wall"))
        .stdout(predicates::str::contains("owns-def"));

    // Confirm the exact field this row must carry via `--json`, rather
    // than a brittle match on the table's own column widths.
    let json = run_json(&game, &["changes", "fixture.moda"]);
    let items = json["items"].as_array().expect("items array");
    let wall_row = items
        .iter()
        .find(|row| row["def_ref"] == "ThingDef/Fixture_Wall" && row["kind"] == "owns-def")
        .unwrap_or_else(|| panic!("no owns-def row for ThingDef/Fixture_Wall in {items:?}"));
    assert_eq!(wall_row["other_touchers"], 1);
    // The owns-def row's own `finding_keys` names the seeded
    // `DefOverride` — an assertion that a bare `other_touchers`
    // count could never catch (it says *how many* touch it, not *which*
    // finding that contention actually raised).
    let finding_keys = wall_row["finding_keys"]
        .as_array()
        .expect("finding_keys array");
    assert!(
        finding_keys
            .iter()
            .any(|key| key.as_str().unwrap_or_default().contains("def_override")),
        "expected a def_override finding key in {finding_keys:?}"
    );

    // The texture-asset row (both mods ship `Textures/Things/Fixture_Wall.png`)
    // carries its own finding key too.
    let texture_row = items
        .iter()
        .find(|row| {
            row["kind"]
                .as_str()
                .unwrap_or_default()
                .starts_with("overrides-asset")
        })
        .unwrap_or_else(|| panic!("no overrides-asset row in {items:?}"));
    let texture_finding_keys = texture_row["finding_keys"]
        .as_array()
        .expect("finding_keys array");
    assert!(
        !texture_finding_keys.is_empty(),
        "expected the contested texture row to carry at least one finding key, got {items:?}"
    );
}

#[test]
fn defs_inspect_prints_owners_winner_and_effective_field_provenance() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("inspect").arg("ThingDef/Fixture_Wall");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("fixture.moda"))
        .stdout(predicates::str::contains("fixture.modb (winner)"))
        .stdout(predicates::str::contains("winner: fixture.modb"))
        .stdout(predicates::str::contains("statBases/MaxHitPoints"))
        .stdout(predicates::str::contains("owner (fixture.modb)"));

    let json = run_json(&game, &["inspect", "ThingDef/Fixture_Wall"]);
    assert_eq!(json["winner"], "fixture.modb");
    let owners = json["owners"].as_array().expect("owners array");
    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0]["mod_id"], "fixture.moda");
    assert_eq!(owners[1]["mod_id"], "fixture.modb");
    let fields = json["fields"].as_array().expect("fields array");
    let hit_points = fields
        .iter()
        .find(|field| field["path"] == "statBases/MaxHitPoints")
        .expect("statBases/MaxHitPoints field");
    assert_eq!(hit_points["provenance"], "owner (fixture.modb)");

    // The fixture's own `ParentName` chain is two deep — `Fixture_Wall`
    // -> `Fixture_WallBase2` -> `Fixture_WallBase` — so `parents` must
    // carry both links, nearest first, not just the immediate one.
    let parents = json["parents"].as_array().expect("parents array");
    assert_eq!(parents.len(), 2, "expected two parents in {parents:?}");
    assert!(
        parents[0]["def_key"]
            .as_str()
            .unwrap_or_default()
            .contains("Fixture_WallBase2"),
        "nearest parent should be Fixture_WallBase2, got {parents:?}"
    );
    assert!(
        parents[1]["def_key"]
            .as_str()
            .unwrap_or_default()
            .contains("Fixture_WallBase")
            && !parents[1]["def_key"]
                .as_str()
                .unwrap_or_default()
                .contains("Fixture_WallBase2"),
        "furthest parent should be Fixture_WallBase, got {parents:?}"
    );
}

#[test]
fn defs_inspect_a_typed_template_ref_shows_its_own_child() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // `ThingDef/@Fixture_WallBase2` is a `Name`-attributed template with
    // a known `def_type` — the typed template ref form, distinct from
    // the name-only `@<name>` form the next test exercises. Both mods'
    // own `Fixture_Wall` registrations declare `ParentName="Fixture_WallBase2"`
    // — two distinct raw nodes, hence two child entries sharing one
    // `DefKey` (`children` is per-registrant, never deduped by key; see
    // `crates/rim-analyzer/CLAUDE.md`'s `children_by_template` note).
    let json = run_json(&game, &["inspect", "ThingDef/@Fixture_WallBase2"]);
    let children = json["children"].as_array().expect("children array");
    assert_eq!(children.len(), 2, "{children:?}");
    assert!(
        children.iter().all(|child| child["def_key"]
            .as_str()
            .unwrap_or_default()
            .contains("Fixture_Wall")
            && !child["def_key"]
                .as_str()
                .unwrap_or_default()
                .contains("Fixture_WallBase")),
        "{children:?}"
    );
}

#[test]
fn defs_inspect_a_name_only_ref_resolves_across_def_types_with_one_child() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // `@Fixture_WallBase` (no `<def_type>/` prefix) is the name-only ref
    // grammar:
    // resolved by a `Name`-only scan, never a `(def_type, Name)` lookup.
    // Its only direct child is `Fixture_WallBase2` (inspection lists direct
    // children only).
    let json = run_json(&game, &["inspect", "@Fixture_WallBase"]);
    let children = json["children"].as_array().expect("children array");
    assert_eq!(
        children.len(),
        1,
        "Fixture_WallBase's only direct child is Fixture_WallBase2, got {children:?}"
    );
    assert!(
        children[0]["def_key"]
            .as_str()
            .unwrap_or_default()
            .contains("Fixture_WallBase2"),
        "{children:?}"
    );
}

/// `--source suggested` really does report a different winner when the
/// suggested order differs — the differential coverage the `Hard`
/// `ParentTemplate` edge rules out for the test below.
///
/// Current order is `[moda, modb, modc]`, so ModC wins by loading last.
/// The pair rule pulls ModC ahead of ModB, which nothing derived opposes,
/// so the suggested order is `[moda, modc, modb]` and ModB wins instead.
#[test]
fn defs_inspect_source_suggested_reports_a_different_winner_than_current() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    add_unconstrained_third_owner(&game);
    write_third_owner_pair_rule(&game);

    let current = run_json(&game, &["inspect", "ThingDef/Fixture_Wall"]);
    assert_eq!(current["source"], "current");
    assert_eq!(current["winner"], "fixture.modc");

    let suggested = run_json(
        &game,
        &["inspect", "ThingDef/Fixture_Wall", "--source", "suggested"],
    );
    assert_eq!(suggested["source"], "suggested");
    assert_eq!(
        suggested["winner"], "fixture.modb",
        "the rule moves ModC ahead of ModB, so ModB is the one that loads last"
    );
}

/// `--source suggested` runs the sorter rather than reading
/// `ModsConfig.xml`, and reports the winner that order implies.
///
/// ModB's `Fixture_Wall` inherits `Fixture_WallBase2`, which only ModA
/// registers, so a contradicting user pair rule cannot flip the winner to
/// `fixture.moda`: ordering ModB first would make RimWorld log `Could not
/// find parent node named "Fixture_WallBase2"` and drop the def. That is a
/// `Hard` `ParentTemplate` edge, added ahead of every rule layer, so the
/// rule loses and the suggested order agrees with the current one. See
/// [`write_flipping_pair_rule`]'s own doc comment.
#[test]
fn defs_inspect_source_suggested_keeps_a_hard_parent_template_order_over_a_user_rule() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    write_flipping_pair_rule(&game);

    let current = run_json(&game, &["inspect", "ThingDef/Fixture_Wall"]);
    assert_eq!(current["source"], "current");
    assert_eq!(current["winner"], "fixture.modb");

    let suggested = run_json(
        &game,
        &["inspect", "ThingDef/Fixture_Wall", "--source", "suggested"],
    );
    assert_eq!(suggested["source"], "suggested");
    assert_eq!(
        suggested["winner"], "fixture.modb",
        "the user rule asks for an order that would drop ModB's own def; \
         the Hard ParentTemplate edge outranks it"
    );
}

#[test]
fn defs_inspect_an_ambiguous_template_shows_per_child_resolution_not_a_single_winner() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    add_two_registrants_for_an_ambiguous_template(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("defs")
        .arg("inspect")
        .arg("ThingDef/@Fixture_AmbiguousBase");
    path_args(&mut cmd, &game);

    // Text output: the bare `winner:`/`(winner)` labelling must read as a
    // disclosed representative, never as a real answer, and the new
    // "ambiguous template" block must name both registrants and show each
    // one's own children resolving to it — not just the other registrant's.
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "winner: fixture.mody  (representative only",
        ))
        .stdout(predicates::str::contains(
            "fixture.mody (last-loaded representative)",
        ))
        .stdout(predicates::str::contains(
            "ambiguous template: this Name has 2 registrations",
        ))
        .stdout(predicates::str::contains(
            "fixture.modx — 1 known child(ren) resolve here",
        ))
        .stdout(predicates::str::contains("fixture.modchilda"))
        .stdout(predicates::str::contains(
            "fixture.mody — 1 known child(ren) resolve here",
        ))
        .stdout(predicates::str::contains("fixture.modchildb"));

    let json = run_json(&game, &["inspect", "ThingDef/@Fixture_AmbiguousBase"]);
    let ambiguity = &json["template_ambiguity"];
    assert!(
        !ambiguity.is_null(),
        "expected template_ambiguity to be present, got {json:?}"
    );
    let registrants = ambiguity["registrants"]
        .as_array()
        .expect("registrants array");
    assert_eq!(
        registrants,
        &[Value::from("fixture.modx"), Value::from("fixture.mody")]
    );
    let resolutions = ambiguity["resolutions"]
        .as_object()
        .expect("resolutions object");
    assert_eq!(resolutions["fixture.modchilda"], "fixture.modx");
    assert_eq!(resolutions["fixture.modchildb"], "fixture.mody");
}

#[test]
fn defs_search_finds_the_contested_wall() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("search").arg("Wall");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("ThingDef/Fixture_Wall"));

    let json = run_json(&game, &["search", "Wall"]);
    let hits = json.as_array().expect("search results array");
    let wall = hits
        .iter()
        .find(|hit| hit["def_ref"] == "ThingDef/Fixture_Wall")
        .expect("ThingDef/Fixture_Wall in search results");
    assert_eq!(wall["owners"], 2);
}

#[test]
fn defs_search_with_no_match_prints_a_no_match_message() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("search").arg("NoSuchDefAnywhere");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("no defs or templates match"));
}

#[test]
fn defs_changes_kind_overrides_texture_yields_only_the_texture_row() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // The fine-grained `overrides-texture` value
    // must expand to exactly the same single `AssetKind::Texture` row
    // the coarse `overrides-asset` value already includes — the fixture
    // ships no sound/keyed-translation conflict, so this also proves the
    // fine-grained filter doesn't over-match onto `owns-def`/`patches-def`
    // rows.
    let json = run_json(
        &game,
        &["changes", "fixture.moda", "--kind", "overrides-texture"],
    );
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["kind"], "overrides-asset(texture)");

    let coarse = run_json(
        &game,
        &["changes", "fixture.moda", "--kind", "overrides-asset"],
    );
    assert_eq!(coarse["items"], json["items"]);
}

#[test]
fn defs_inspect_an_unknown_ref_exits_non_zero_naming_it_not_found() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("inspect").arg("ThingDef/NoSuchDef");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn defs_inspect_an_unparsable_ref_exits_2_naming_the_missing_separator() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // `Foo` has no `/` at all — a `clap` argument-parsing failure
    // (`DefRef::from_str`'s own `DefRefParseError::MissingSeparator`),
    // never reaching `InspectDef`, so this is `clap`'s own usage-error
    // exit code (2), distinct from the `NotFound` case above (exit 1,
    // an `anyhow::bail!` from inside `run_inspect`).
    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("inspect").arg("Foo");
    path_args(&mut cmd, &game);

    cmd.assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("missing '/' separator"));
}

#[test]
fn defs_inspect_output_is_byte_identical_across_two_runs() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    let run = || {
        let mut cmd = common::rimmerge();
        cmd.arg("defs").arg("inspect").arg("ThingDef/Fixture_Wall");
        path_args(&mut cmd, &game);
        cmd.assert().success().get_output().stdout.clone()
    };

    assert_eq!(run(), run());
}

#[test]
fn defs_inspect_reports_reached_caveats_and_wrapper_flag_on_a_stopped_fold() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    add_unsupported_patcher_and_a_later_patcher(&game);

    let mut cmd = common::rimmerge();
    cmd.arg("defs").arg("inspect").arg("ThingDef/Fixture_Wall");
    path_args(&mut cmd, &game);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("completeness: PARTIAL"))
        .stdout(predicates::str::contains("fixture.modc"))
        .stdout(predicates::str::contains(
            "not reached (an earlier mod's op stopped the fold)",
        ))
        .stdout(predicates::str::contains("inside a wrapper op"));

    let json = run_json(&game, &["inspect", "ThingDef/Fixture_Wall"]);
    assert_eq!(json["completeness"], "partial");
    let stopped_at = json["stopped_at"].as_str().expect("stopped_at present");
    assert!(stopped_at.contains("fixture.modc"), "{stopped_at}");
    assert!(
        stopped_at.contains("unsupported operation class"),
        "{stopped_at}"
    );

    let patchers = json["patchers"].as_array().expect("patchers array");

    // `fixture.modc` is the stopper's own mod: `reached: true` (it *was*
    // attempted — it's the op that failed), `replay_ok: false` (the fold
    // stopped on its own op).
    let modc = patchers
        .iter()
        .find(|patcher| patcher["mod_id"] == "fixture.modc")
        .expect("fixture.modc patcher");
    assert_eq!(modc["reached"], true, "{modc:?}");
    assert_eq!(modc["replay_ok"], false, "{modc:?}");
    assert!(modc["caveats"].is_array());

    // `fixture.modd` loads after the stopper: its own op was never
    // attempted (`reached: false`), so `replay_ok` stays the "not
    // disproven" `true` rather than a verified pass — and its one op,
    // wrapped in a `PatchOperationSequence`, carries `is_wrapped: true`.
    let modd = patchers
        .iter()
        .find(|patcher| patcher["mod_id"] == "fixture.modd")
        .expect("fixture.modd patcher");
    assert_eq!(modd["reached"], false, "{modd:?}");
    assert_eq!(modd["replay_ok"], true, "{modd:?}");
    let modd_ops = modd["ops"].as_array().expect("ops array");
    assert_eq!(modd_ops.len(), 1, "{modd_ops:?}");
    assert_eq!(modd_ops[0]["is_wrapped"], true, "{modd_ops:?}");
}
