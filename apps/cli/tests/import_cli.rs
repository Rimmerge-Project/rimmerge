//! `rimmerge import`'s cache-aware surface end to end — `--from-cache`,
//! `--user-rules`, and the resolution order (`--rimsort-dir` wins over a populated
//! cache) — against a scratch copy of `rim-io`'s `merge_game` fixture and
//! hand-built RimSort/cache directories, never the network and never the
//! real profile.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

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

fn rules_json(game: &ScratchGame) -> serde_json::Value {
    let raw = fs::read_to_string(game.profile_dir.join("rules.json"))
        .unwrap_or_else(|error| panic!("read rules.json: {error}"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("rules.json must be valid JSON: {error}"))
}

/// A RimSort database directory with the classic three-file layout
/// (`userRules.json` directly under it, the other two under their own
/// subfolders) — `--rimsort-dir`'s own expected shape.
fn write_rimsort_dir(dir: &Path, community_loads_bottom: bool) {
    fs::create_dir_all(dir.join("Community-Rules-Database"))
        .unwrap_or_else(|error| panic!("create Community-Rules-Database dir: {error}"));
    fs::create_dir_all(dir.join("Steam-Workshop-Database"))
        .unwrap_or_else(|error| panic!("create Steam-Workshop-Database dir: {error}"));
    fs::write(dir.join("userRules.json"), r#"{"rules":{}}"#)
        .unwrap_or_else(|error| panic!("write userRules.json: {error}"));
    let community_body = if community_loads_bottom {
        r#"{"rules":{"fixture.moda":{"loadBottom":{"value":true}}}}"#
    } else {
        r#"{"rules":{}}"#
    };
    fs::write(
        dir.join("Community-Rules-Database")
            .join("communityRules.json"),
        community_body,
    )
    .unwrap_or_else(|error| panic!("write communityRules.json: {error}"));
    fs::write(
        dir.join("Steam-Workshop-Database").join("steamDB.json"),
        r#"{"database":{}}"#,
    )
    .unwrap_or_else(|error| panic!("write steamDB.json: {error}"));
}

/// The global rule-database cache's own flat layout (no subfolders,
/// `rim_io::databases_dir`'s own convention) — `communityRules.json`'s
/// content matches [`write_rimsort_dir`]'s own `community_loads_bottom`
/// body, so the two sources are directly comparable in
/// `from_cache_produces_the_same_rule_counts_as_rimsort_dir`.
fn write_cache_dir(dir: &Path, community_loads_bottom: bool) {
    fs::create_dir_all(dir).unwrap_or_else(|error| panic!("create cache dir: {error}"));
    let community_body = if community_loads_bottom {
        r#"{"rules":{"fixture.moda":{"loadBottom":{"value":true}}}}"#
    } else {
        r#"{"rules":{}}"#
    };
    fs::write(dir.join("communityRules.json"), community_body)
        .unwrap_or_else(|error| panic!("write communityRules.json: {error}"));
    // `fetch_steam_workshop` defaults to `true` (`NetworkPolicy::default`),
    // but a source is only imported when its cache file also exists, so
    // leaving `steamDB.json` absent keeps these tests on a deterministic
    // community-only comparison — and exercises
    // `should_import_from_cache`'s "no cache file" arm live, through the
    // binary, not only in `commands/import.rs`'s own unit tests.
}

/// Seeds `<profile_dir>/rules.json` with one already-imported
/// `RimSortUser` placement rule — the state the user-rules preservation
/// test below must find untouched afterward.
fn seed_imported_user_placement(game: &ScratchGame) {
    seed_imported_placement(game, "rim_sort_user");
}

/// Seeds `<profile_dir>/rules.json` with one already-imported placement
/// rule of the given `origin` (a `RuleOrigin`'s own snake_case JSON
/// spelling, e.g. `"rim_sort_community"`/`"rim_sort_user"`).
fn seed_imported_placement(game: &ScratchGame, origin: &str) {
    fs::create_dir_all(&game.profile_dir)
        .unwrap_or_else(|error| panic!("create profile dir: {error}"));
    fs::write(game.profile_dir.join("rules.json"),
        format!(r#"{{
                "version": 2,
                "pairs": [],
                "placements": [
                    {{"mod_id": "fixture.moda", "placement": "bottom", "origin": "{origin}", "comment": null}}
                ],
                "incompatibles": [],
                "tag_rules": [],
                "manual_tags": [],
                "settings": {{
                    "threshold": 80,
                    "enforce_soft": false,
                    "enforce_awareness": false,
                    "suggest_merge_when_clean": true,
                    "tie_break": "rebuild",
                    "use_imported_pairs": false,
                    "use_imported_placements": true
                }}
            }}"#
        ))
    .unwrap_or_else(|error| panic!("seed rules.json: {error}"));
}

#[test]
fn from_cache_produces_the_same_rule_counts_as_rimsort_dir() {
    let temp_dir = tempdir().expect("tempdir");

    let rimsort_dir = temp_dir.path().join("rimsort_db");
    write_rimsort_dir(&rimsort_dir, true);
    let game_a = scratch_game(&temp_dir.path().join("a"));
    let mut cmd = common::rimmerge();
    cmd.arg("import").arg("--rimsort-dir").arg(&rimsort_dir);
    path_args(&mut cmd, &game_a);
    cmd.assert().success();
    let placements_a = rules_json(&game_a)["placements"]
        .as_array()
        .expect("placements array")
        .len();

    let cache_dir = temp_dir.path().join("cache");
    write_cache_dir(&cache_dir, true);
    let game_b = scratch_game(&temp_dir.path().join("b"));
    let mut cmd = common::rimmerge();
    cmd.arg("import")
        .arg("--from-cache")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game_b);
    cmd.assert().success();
    let placements_b = rules_json(&game_b)["placements"]
        .as_array()
        .expect("placements array")
        .len();

    assert_eq!(
        placements_a, placements_b,
        "the same communityRules.json body must import the same placement count \
         from either source"
    );
    assert_eq!(
        placements_a, 1,
        "sanity: the fixture body does assert one placement"
    );
}

#[test]
fn user_rules_flag_alone_imports_user_rules_without_touching_existing_community_rules() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    // A pre-existing *community*-origin placement — distinct from the
    // origin `--user-rules` is about to import, so a survivor proves the
    // community source was genuinely left alone rather than merely
    // replaced by an equally-sized result.
    seed_imported_placement(&game, "rim_sort_community");

    let user_rules_path = temp_dir.path().join("userRules.json");
    fs::write(
        &user_rules_path,
        r#"{"rules":{"fixture.modb":{"loadBottom":{"value":true}}}}"#,
    )
    .expect("write userRules.json");

    let mut cmd = common::rimmerge();
    cmd.arg("import")
        .arg("--user-rules")
        .arg(&user_rules_path)
        // No `--rimsort-dir`, and an empty cache directory — community/
        // steam must stay exactly as they were.
        .arg("--cache-dir")
        .arg(temp_dir.path().join("empty_cache"));
    path_args(&mut cmd, &game);
    cmd.assert().success().stdout(predicates::str::contains(
        "community rules       : not imported",
    ));

    let placements_after = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(
        placements_after.len(),
        2,
        "the pre-existing community placement must survive alongside the newly \
         imported user rule: {placements_after:?}"
    );
    let origins: Vec<&str> = placements_after
        .iter()
        .map(|p| p["origin"].as_str().expect("origin string"))
        .collect();
    assert!(origins.contains(&"rim_sort_community"), "{origins:?}");
    assert!(origins.contains(&"rim_sort_user"), "{origins:?}");
}

/// The import's user-rules preservation guarantee, asserted end to end at the
/// command line — the exact invocation a user with no RimSort install
/// will type once community/steam rules already come from the cache by
/// default: `import --from-cache` with neither `--user-rules` nor
/// `--rimsort-dir` must never delete rules a *previous* import already
/// wrote to `rules.json`.
#[test]
fn from_cache_with_no_user_rules_and_no_rimsort_dir_leaves_previously_imported_user_rules_untouched()
 {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_imported_user_placement(&game);

    let cache_dir = temp_dir.path().join("cache");
    write_cache_dir(&cache_dir, false);

    let mut cmd = common::rimmerge();
    cmd.arg("import")
        .arg("--from-cache")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    cmd.assert().success().stdout(predicates::str::contains(
        "user rules            : not imported",
    ));

    let placements_after = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert_eq!(
        placements_after.len(),
        1,
        "the previously-imported RimSortUser placement must still be there: \
         {placements_after:?}"
    );
    assert_eq!(
        placements_after[0]["origin"].as_str(),
        Some("rim_sort_user"),
        "{placements_after:?}"
    );
}

#[test]
fn explicit_rimsort_dir_wins_over_a_populated_cache() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());

    // The cache has the placement rule; the RimSort dir does not — if
    // the cache won, the import would produce one placement, not zero.
    let cache_dir = temp_dir.path().join("cache");
    write_cache_dir(&cache_dir, true);
    let rimsort_dir = temp_dir.path().join("rimsort_db");
    write_rimsort_dir(&rimsort_dir, false);

    let mut cmd = common::rimmerge();
    cmd.arg("import")
        .arg("--rimsort-dir")
        .arg(&rimsort_dir)
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    cmd.assert().success();

    let placements = rules_json(&game)["placements"]
        .as_array()
        .expect("placements array")
        .clone();
    assert!(
        placements.is_empty(),
        "an explicit --rimsort-dir must win over the populated cache: {placements:?}"
    );
}
